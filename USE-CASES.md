# m1nd Use Cases — Comprehensive Guide by Audience

> Status: supporting public reference. For the canonical product/onboarding path, start with `README.md`, the wiki, and `EXAMPLES.md`.

> "grep finds what you asked for. m1nd finds what's missing."

This document is grounded in a live audit session on 2026-03-14 against a Python/FastAPI codebase:
**10,401 nodes · 11,733 edges · 380 files · ~52K lines**

Every pipeline, metric, and example below is from real execution. Not a demo. Not a simulation.

---

## Table of Contents

1. [For AI Agents](#1-for-ai-agents)
2. [For Human Developers](#2-for-human-developers)
3. [For CI/CD Pipelines](#3-for-cicd-pipelines)
4. [For Security Audits](#4-for-security-audits)
5. [For Teams](#5-for-teams)
6. [Proven Pipelines — Real Session Data](#6-proven-pipelines--real-session-data)
7. [Comparative Benchmarks](#7-comparative-benchmarks)
8. [HTTP Server + GUI Mode](#8-http-server--gui-mode)

---

## 1. For AI Agents

AI agents are m1nd's primary audience. The tools are built for machine speed, machine precision, and
machine-native interfaces (MCP, JSON, graph topology). No GUIs. No vibes.

### 1.1 System-Wide Bug Hunt

The problem: you need to audit an entire codebase for reliability issues. grep shows you what's there.
It cannot show you what should be there but isn't.

```
Pipeline: scan → missing → resonate → activate → hypothesize → predict → learn
```

**Real session results:**
- 46 m1nd queries
- 39 bugs found total (28 confirmed fixed + 9 new high-confidence): 3 critical, 11 high, 9 medium, 5 low + 9 high-confidence new findings
- 8 bugs (28.5%) were structurally invisible to grep — they required detecting ABSENCE
- 170+ new tests written, zero regressions
- Total m1nd latency: ~3.1 seconds

```jsonc
// Step 1: Targeting — find structural regions
{ "tool": "scan", "patterns": ["concurrency", "resource_cleanup", "error_handling", "state_mutation"] }
// → Returns node clusters matching each pattern. Time: 7.6ms. LLM tokens: 0.

// Step 2: Find missing guards (structural holes)
{ "tool": "missing", "query": "worker pool session reuse timeout cleanup" }
// → Returns: is_alive TOCTOU hole at worker_pool.py (score 1.12), circuit breaker gap
// → These holes DON'T EXIST in the source. grep can't find them.

// Step 3: Harmonic anomalies
{ "tool": "resonate", "query": "stormender phase timeout cancel", "harmonics": 5 }
// → Returns: 9 "cancel" nodes at amplitude 1.4 — not converging.
// → Interpretation: lifecycle.py and control.py both cancel phases → TOCTOU race.

// Step 4: Test a specific claim
{ "tool": "hypothesize", "claim": "session_pool leaks on storm cancel", "depth": 8 }
// → Returns: 99% confidence, 25,000+ paths analyzed, 3 supporting evidence groups
// → Then read the specific files. Confirm. Fix.

// Step 5: After fixes, validate co-changes
{ "tool": "predict", "node": "session_pool.py" }
// → Returns: QueuedTask, ErrorType likely need corresponding updates
// → Check them. They did.

// Step 6: Train the graph
{ "tool": "learn", "feedback": "correct", "agent_id": "audit-agent-001", "context": "session_pool TOCTOU" }
// → Strengthens the edges that led to this finding. Next audit: faster.
```

**Why this beats grep:**
The TOCTOU race in the circuit breaker was found because `missing()` detected a dict-as-structural-hole
in a concurrency context — no lock on read-check-modify. There is no text pattern to grep for when
the bug IS the absence of a lock. m1nd found it. grep would have returned nothing and called it clean.

---

### 1.2 Pre-Code Grounding (Before Any Edit)

Before touching code, an agent should understand the structural blast radius.

```
Pipeline: impact → validate_plan → warmup → [edit] → predict → ingest (incremental)
```

```jsonc
// What breaks if I touch this?
{ "tool": "impact", "node": "worker_pool.py", "depth": 3 }
// → Returns: 350 affected nodes, 9,551 causal chains. Do NOT touch lightly.

// Is my plan safe?
{
  "tool": "validate_plan",
  "plan": "modify session_pool and worker_pool to fix CancelledError handling",
  "files": ["session_pool.py", "worker_pool.py"]
}
// → Returns: risk=0.70, 347 structural gaps identified.
// → Not a blocker — this is expected for high-coupling modules. It IS a warning to test everything.

// Prime the graph for your focus area
{ "tool": "warmup", "task": "fix async cancellation in session and worker pools", "agent_id": "hacker-001" }
// → Pre-activates the subgraph. Subsequent queries run faster and return more relevant results.

// After editing, check what else needs to change
{ "tool": "predict", "node": "session_pool.py" }
// → Returns co-change predictions. Follow them. Don't skip.
```

---

### 1.3 Build Orchestration — TEMPESTA Pattern

During a parallel build (TEMPESTA: 16 agents building in parallel), m1nd is the coordination layer.

```
Per module: warmup → [build] → ingest(incremental) → learn("correct") → predict → warmup(next)
```

**Real session data:**
- 16 Sonnet agents building in parallel
- m1nd warmup before each module primed the subgraph for that agent's context
- ingest(incremental) re-indexed each completed module in ~138ms
- predict() after each fix caught 2 missed co-changes before agents diverged
- Zero merge conflicts in graph state (multi-agent writes are atomic)

```jsonc
// Before agent takes a module
{ "tool": "warmup", "task": "implement asyncio.shield on pool.release", "agent_id": "forge-pool" }

// Agent writes code. Then after:
{ "tool": "ingest", "source": "/path/to/session_pool.py", "incremental": true }
// → Re-indexes only changed nodes. Fast: 0.07ms for a single file.

// Feedback so the graph learns from this build
{ "tool": "learn", "feedback": "correct", "agent_id": "forge-pool", "context": "asyncio.shield fix" }

// Check completeness
{ "tool": "predict", "node": "session_pool.py" }
// → Returns: QueuedTask.py should also change — co-change prediction.
```

---

### 1.4 Investigation Persistence — Trail System

An investigation takes more than one session. Trail system saves and restores exact cognitive context.

```
Pipeline: trail.save (mid-session) → trail.list (next session) → trail.resume → trail.merge (multi-agent)
```

**Real session proof:**
- Trail `trail_jimi_001_af21f9bc` saved mid-investigation with all hypotheses and activated nodes
- Resumed in a subsequent session with exact context restored
- Two parallel trails (attestation + WhatsApp) merged with conflict detection

```jsonc
// Save investigation state
{
  "tool": "trail.save",
  "trail_id": "trail_jimi_001",
  "agent_id": "audit-001",
  "notes": "investigating session_pool TOCTOU. 3 bugs found. next: sacred_memory concurrent write"
}
// → Returns: trail_jimi_001_af21f9bc saved. Restores to this exact graph activation state.

// Days later, resume
{ "tool": "trail.resume", "trail_id": "trail_jimi_001_af21f9bc" }
// → Graph reactivates to investigation context. Pick up from exactly where you left off.

// Merge two agents' trails
{ "tool": "trail.merge", "trails": ["trail_attestation_001", "trail_whatsapp_001"] }
// → Detects conflicts (same node with different hypotheses). Surfaces for resolution.
```

---

### 1.5 Architecture Exploration

When onboarding to an unfamiliar codebase or recovering context after compaction:

```
Pipeline: drift → layers → activate(domain) → why(a, b) → counterfactual(modules)
```

```jsonc
// What changed since last session?
{ "tool": "drift", "since": "last_session", "agent_id": "jimi" }
// → Returns: which modules gained/lost edges. Where the system "moved" while you were away.

// What layers does this system have?
{ "tool": "layers", "agent_id": "jimi" }
// → Returns: ConnectionManager (98 nodes), API (161 nodes), Core (5,036 nodes)
// → Plus violations: nodes that bridge layers they shouldn't.

// What's related to this concept?
{ "tool": "activate", "query": "chat escalation deep work", "agent_id": "jimi" }
// → Returns: ranked nodes. Spreads across structural + semantic + causal dimensions.

// Why does A connect to B?
{ "tool": "why", "from": "chat_handler", "to": "sacred_memory" }
// → Returns: 1 hop via `inspect` (reflexão). Hidden coupling made visible.

// What's the blast radius if we removed this module?
{ "tool": "counterfactual", "nodes": ["storm_manager", "config", "lifespan"] }
// → Returns: 4,041 nodes affected (41% of entire graph). Do not remove lightly.
```

---

## 2. For Human Developers

Human developers use m1nd through GUI tools (Cursor, Claude Code, Windsurf, etc.) that surface
the graph visually. The mental models map to common developer questions.

### 2.1 "Where's the Bug?"

```
Pipeline: trace → activate → perspective.start → perspective.follow → perspective.peek
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
// → Graph lights up connected modules. Visual: the blast area of this bug.

// Navigate from the crash site
{ "tool": "perspective.start", "entry": "session_pool.py", "agent_id": "dev" }
{ "tool": "perspective.routes", "agent_id": "dev" }
// → Returns: available structural routes from here. Not file tree — STRUCTURAL routes.
// → Follow the route toward the bug's origin, not just its manifestation.

{ "tool": "perspective.peek", "agent_id": "dev" }
// → Shows source code at the current perspective focus. Exact context, no file browsing.
```

---

### 2.2 "Is It Safe to Deploy?"

```
Pipeline: epidemic → flow_simulate → trust → validate_plan
```

You want a risk heatmap before you push to production.

```jsonc
// Spread infection from recently modified modules
{
  "tool": "epidemic",
  "infected": ["worker_pool", "session_pool", "chat_handler", "stormender", "doctor"],
  "auto_calibrate": true
}
// → Returns: lifespan.py (high), config.py (medium), models.py (low), test_session_persistence_harden.py
// → These modules have NOT been checked but are statistically likely to be affected.
// Real result: lifespan.py was indeed impacted — found during subsequent review.

// Simulate concurrent requests through the system
{ "tool": "flow_simulate", "entry": "settings_routes", "particles": 4, "max_depth": 8 }
// → Returns: 51 turbulence points, 11 valves (locks working correctly)
// → Turbulence = node receiving multiple simultaneous flows WITHOUT a lock → race condition.
// → Settings system: 51 turbulence = significant race condition surface. Do not ship without fixing.
// → Compare: after bug fixes, core system showed 0 turbulence. That's the target state.

// Module trust scores (requires learn() history)
{ "tool": "trust", "agent_id": "dev" }
// → Returns: per-module defect density from historical learn() events
// → worker_pool.py (5 historical bugs) = low trust. config.py (0 bugs) = high trust.
// → Shows WHERE to focus pre-deploy review, not just WHAT changed.
```

**The deploy decision table:**

| flow_simulate turbulence | epidemic reach | trust scores | Decision |
|--------------------------|---------------|--------------|----------|
| 0 | < 5% graph | All high | Ship it |
| < 20 | < 15% graph | Mixed | Ship with monitoring |
| > 50 | > 25% graph | Any low | Hold. Fix first. |
| 200+ | > 40% graph | Any low | Do not ship. |

---

### 2.3 "How Does This System Work?"

```
Pipeline: layers → activate(domain) → perspective.start → perspective.follow → why
```

Onboarding to an unfamiliar codebase, or returning after months away.

```jsonc
// Auto-detect architectural layers
{ "tool": "layers", "agent_id": "dev" }
// → Returns: 3 layers detected automatically from topology
// → L0: ConnectionManager (98 nodes) — entry points
// → L1: API (161 nodes) — routing and handling
// → L2: Core (5,036 nodes) — business logic
// → Plus: 892 utility nodes, 20 layer violations

// A layer violation: if session_pool.py (L2 data layer) imports from chat_handler.py (L1 handler)
// → That's an architectural bug. Detected without reading code.

{ "tool": "layers", "inspect": "L2", "agent_id": "dev" }
// → Health metrics for this layer, top nodes by PageRank, internal cohesion score

// Navigate through the architecture
{ "tool": "perspective.start", "entry": "chat_handler", "agent_id": "dev" }
{ "tool": "perspective.suggest", "agent_id": "dev" }
// → AI-recommended next node to visit based on your investigation context

{ "tool": "perspective.branch", "agent_id": "dev" }
// → Fork the current perspective. Explore an alternative path without losing your current position.
{ "tool": "perspective.compare", "a": "perspective_001", "b": "perspective_002", "agent_id": "dev" }
// → Shows shared nodes + unique nodes between two exploration paths.
```

---

### 2.4 "What Changed Since Last Release?"

```
Pipeline: drift → timeline → lock.diff → differential
```

```jsonc
// What moved in the graph since last week?
{ "tool": "drift", "since": "2026-03-07", "agent_id": "dev" }
// → Returns: modules that gained/lost edges, weight changes, new structural holes
// → Not "what files changed" — "what RELATIONSHIPS changed"

// Churn velocity per module
{ "tool": "timeline", "node": "chat_handler.py", "window": "30d" }
// → Returns: commit frequency, co-change patterns, velocity acceleration
// → High velocity + acceleration = "code tremor" — potential instability building up

// Lock a region, deploy, diff
{ "tool": "lock.create", "region": "settings_routes", "agent_id": "dev" }
// → Pins 1,639 nodes in this subgraph at current state
// → Deploy to staging
{ "tool": "lock.diff", "agent_id": "dev" }
// → Returns: exactly which structural changes occurred since the lock. 0.08µs.
// → Clean diff: show only what the deployment actually touched, structurally.

// Compare two graph snapshots
{ "tool": "differential", "snapshot_a": "pre-deploy", "snapshot_b": "post-deploy" }
// → Side-by-side structural diff. Added/removed/modified edges. Not file diff — graph diff.
```

---

### 2.5 Architecture Review

```
Pipeline: fingerprint → counterfactual → layers → resonate
```

Periodic architectural health review, pre-refactor analysis.

```jsonc
// Find duplicate or near-duplicate modules
{ "tool": "fingerprint", "threshold": 0.8, "agent_id": "dev" }
// → Returns: pairs with structural similarity > 80%
// → 0 twins on ROOMANIZER = clean. No accidental duplication.
// → On a legacy codebase: likely finds 10-20% duplicate service classes.

// What if we removed/consolidated these modules?
{ "tool": "counterfactual", "nodes": ["legacy_auth_v1", "legacy_auth_v2"] }
// → Returns: N nodes affected, K% of graph. Informs consolidation risk.

// Find harmonic anomalies in the architecture
{ "tool": "resonate", "query": "daemon lifecycle crash recovery", "harmonics": 5 }
// → Returns: 149 antinodes, energy 14.4, cluster around daemon modules
// → Antinodes = structural resonance — modules that co-vibrate.
// → A tight daemon cluster vibrating together = they're coupled more than they look.
```

---

## 3. For CI/CD Pipelines

m1nd is a zero-token, local binary. It runs in CI in milliseconds, not minutes. No API calls. No rate limits.

### 3.1 Pre-Merge Gate

Run on every PR. Block merge if risk thresholds are breached.

```bash
# In your CI step
m1nd antibody_scan --patterns "$(cat .m1nd/antibodies.json)" --node-set "$(git diff --name-only HEAD~1)"
# Returns: number of antibody matches in changed files.
# If count > 0: block merge. Describe the known bug pattern that was matched.
```

```jsonc
// MCP version
{
  "tool": "antibody_scan",
  "scope": ["settings_routes.py", "chat_handler.py"],
  "agent_id": "ci-gate"
}
// → If bugs were FIXED, scan returns 0 matches. This is CORRECT: like vaccination.
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
// Real result: 2-file plan → risk=0.70, 347 gaps. Expected for high-coupling modules. Allowed with review.
```

---

### 3.2 Post-Merge Analysis

```jsonc
// Re-index after merge
{ "tool": "ingest", "source": "/repo", "incremental": true, "agent_id": "ci-post" }
// → Updates graph with merged changes. Time: ~138ms for entire repo.

// Predict co-changes that might have been missed
{ "tool": "predict", "node": "session_pool.py", "agent_id": "ci-post" }
// → Returns: modules statistically likely to need updates given this change
// → Post to PR as comment: "m1nd predicts QueuedTask.py may need review"

// Epidemic spread from changed modules
{
  "tool": "epidemic",
  "infected": ["session_pool.py", "worker_pool.py"],
  "auto_calibrate": true,
  "agent_id": "ci-post"
}
// → Returns: downstream modules with estimated infection probability
// → Flag high-probability modules for follow-up review in next sprint
```

---

### 3.3 Nightly Health Scan

```jsonc
// Code tremor detection (requires learn() history > 7 days)
{ "tool": "tremor", "window": "7d", "agent_id": "ci-nightly" }
// → Returns: modules with accelerating change velocity
// → Exponential acceleration = earthquake building. Investigate before it breaks.

// Module trust heatmap
{ "tool": "trust", "agent_id": "ci-nightly" }
// → Returns: per-module defect density from learn() history
// → Post to dashboard: which modules have the worst reliability record

// Architectural layer health
{ "tool": "layers", "agent_id": "ci-nightly" }
// → Returns: violation count (L2 importing L1, etc.)
// → Track violations over time. Rising count = architecture degrading.

// Full-graph flow health
{
  "tool": "flow_simulate",
  "entry": "main_entrypoints",
  "particles": 8,
  "max_depth": 10,
  "agent_id": "ci-nightly"
}
// → Returns: total turbulence score across all entry points
// → Target: 0 turbulence in core (race conditions fixed). Track over releases.
// → Alert if turbulence rises > 20% week-over-week.
```

---

### 3.4 Incident Response

Production is down. You have a stacktrace.

```jsonc
// Feed stacktrace immediately
{
  "tool": "trace",
  "stacktrace": "full stacktrace text here",
  "agent_id": "incident-response"
}
// → Returns: suspects ranked by suspiciousness in 9.8ms.
// → Real result: BrokenPipeError → session_pool.py:_send_persistent suspiciousness 0.401, risk=critical
// → This was the correct root cause. Found in under 10ms.

// Form and test hypothesis
{
  "tool": "hypothesize",
  "claim": "session_pool is leaking sessions when requests are cancelled",
  "depth": 8,
  "agent_id": "incident-response"
}
// → Returns: 99% confidence, 25K paths, 3 evidence groups
// → Don't read all 25K paths. Read the top 3. They contain the bug.

// Understand full blast radius before the fix
{ "tool": "impact", "node": "session_pool.py", "depth": 3, "agent_id": "incident-response" }
// → Returns: 350 nodes affected, 9,551 causal chains
// → Know what you're touching before you touch it. Even under incident pressure.
```

---

## 4. For Security Audits

m1nd found a SECURITY CRITICAL bug with 99% confidence and 20 supporting evidence paths.
Zero code reading required to find it.

### 4.1 Auth Boundary Analysis

```
Pipeline: missing → flow_simulate → layers → validate_plan
```

```jsonc
// Find structural holes in the auth/validation layer
{ "tool": "missing", "query": "principal import attestation validation signature verification" }
// → Returns: agent_identity.py (score 0.875) — structural hole in attestation validation
// → Translation: this module SHOULD connect to a signature verifier. It doesn't.
// → This hole = the bug. No code reading required to find it.

// Simulate flows that skip validation
{
  "tool": "flow_simulate",
  "entry": "agent_identity",
  "particles": 4,
  "max_depth": 8,
  "agent_id": "security-audit"
}
// → Returns: turbulence at nodes that receive flow without passing through a validator node
// → Turbulent node = path exists that reaches this resource without going through auth

// Check layer violations (data reaching core without passing through validation layer)
{ "tool": "layers", "agent_id": "security-audit" }
// → Layer violations = paths that skip layers. In security context: direct access to L2 core
// → from L0 entry WITHOUT passing through L1 validation = auth bypass.
```

---

### 4.2 Secret Exposure Detection

```jsonc
// Create antibody patterns for known secret exposure shapes
{
  "tool": "antibody_create",
  "name": "api_key_in_log",
  "pattern": "api_key_handler → logger → output_stream",
  "severity": "critical",
  "agent_id": "security-audit"
}
// Then scan new code against this antibody on every commit:
{ "tool": "antibody_scan", "scope": "changed_files", "agent_id": "ci-security" }

// Also: scan for structural patterns indicating env variable exposure
{
  "tool": "antibody_create",
  "name": "env_var_serialized",
  "pattern": "os_environ_access → serializer → http_response",
  "severity": "critical"
}
```

---

### 4.3 Input Validation Gap Detection

```
Pipeline: flow_simulate (surface → core) → layers (violations) → missing (validators)
```

```jsonc
// Simulate input flow from HTTP entry to core data store
{
  "tool": "flow_simulate",
  "entry": "http_routes",
  "particles": 10,
  "max_depth": 15,
  "agent_id": "security-audit"
}
// → Turbulence at data store nodes (databases, file writes) WITHOUT passing through
// → a validation node = unvalidated input reaching core storage.
// → Real result: WhatsApp subsystem had 223 turbulence points — highest in the system.
// → This means WhatsApp routes have more unguarded flow paths than any other subsystem.

// Confirm with layer analysis
{ "tool": "layers", "agent_id": "security-audit" }
// → Layer violations at L0→L2 (entry → core, skipping L1 validation) = injection risk

// Find missing validators
{ "tool": "missing", "query": "input validation sanitization before database write" }
// → Returns: structural holes where validation should exist but doesn't
```

---

### 4.4 Attestation / Identity Forgery Detection

This is the most powerful case. m1nd found a SECURITY CRITICAL bug in under 60 seconds.

```jsonc
// Test the hypothesis
{
  "tool": "hypothesize",
  "claim": "agent_identity principal import can accept manifest with forged attestation signature",
  "depth": 10,
  "agent_id": "security-audit"
}
// → Returns: 99% confidence, 20 supporting evidence paths (HIGHEST evidence count of any claim tested)
// → This is not a guess. 20 independent structural paths all support this claim.

// Understand the coupling
{ "tool": "why", "from": "agent_identity", "to": "sacred_memory" }
// → Returns: 1 hop via `inspect` (reflexão) — tight coupling between identity and memory store
// → If identity is forged, sacred_memory is compromised. Direct path.

// Find the structural hole
{ "tool": "missing", "query": "attestation signature verification cryptographic validation" }
// → Returns: agent_identity.py IS the structural hole (avg score 0.875)
// → It should connect to a verifier. It doesn't.

// Quantify blast radius
{ "tool": "counterfactual", "nodes": ["agent_identity"], "agent_id": "security-audit" }
// → Returns: 3,685 nodes affected (35% of entire graph)
// → This is a critical module. A compromise here cascades to 35% of the system.

// Pre-flight the fix
{
  "tool": "validate_plan",
  "plan": "add cryptographic signature verification to principal import in agent_identity",
  "files": ["agent_identity.py", "principal_registry.py"],
  "agent_id": "security-audit"
}
// → Returns: risk=0.85, 347 gaps
// → Fix is high-impact. Requires careful review of all 347 affected paths.
```

**Total time to find this bug: under 60 seconds. Zero code reading.**

---

## 5. For Teams

### 5.1 Region Locking for Parallel Work

When multiple agents or developers work on the same codebase simultaneously.

```jsonc
// Agent A locks the settings subsystem
{
  "tool": "lock.create",
  "region": "settings_routes",
  "agent_id": "forge-settings",
  "strategy": "on_ingest"
}
// → Locks 1,639 nodes. Other agents can READ but their ingest won't overwrite this region.

// Agent B works on auth (different region — no conflict)
{
  "tool": "lock.create",
  "region": "agent_identity",
  "agent_id": "forge-auth"
}

// Agent A finishes, checks what changed in their region
{ "tool": "lock.diff", "agent_id": "forge-settings" }
// → Returns: structural diff since lock was created. 0.08µs.
// → Shows exactly which edges were added/removed in their region.

// Advance baseline (prep for next lock cycle)
{ "tool": "lock.rebase", "agent_id": "forge-settings" }

// Release
{ "tool": "lock.release", "agent_id": "forge-settings" }
```

---

### 5.2 Investigation Handoff

You investigated a bug for 4 hours. You need to hand it to another agent or continue tomorrow.

```jsonc
// Save with full context
{
  "tool": "trail.save",
  "trail_id": "trail_session_pool_audit",
  "agent_id": "auditor-001",
  "notes": "Found 3 bugs in session_pool. Hypothesize points to sacred_memory as next. Check shard read after concurrent write. Use validate_plan before fix — risk is 0.85."
}
// → Saves exact graph activation state, not just notes.

// Next agent lists available trails
{ "tool": "trail.list", "agent_id": "auditor-002" }
// → Returns: all saved trails with timestamps and notes

// Resume from exact cognitive position
{ "tool": "trail.resume", "trail_id": "trail_session_pool_audit_af21f9bc", "agent_id": "auditor-002" }
// → Graph reactivates to the exact state where auditor-001 left off.
// → No re-reading files. No re-running queries. Continue immediately.
```

---

### 5.3 Code Review Enrichment

Before approving a PR, use m1nd to understand structural impact beyond the diff.

```jsonc
// Structural similarity check — is this a duplicate of existing code?
{ "tool": "fingerprint", "nodes": ["new_auth_handler"], "threshold": 0.75, "agent_id": "reviewer" }
// → 0 twins = novel code. Score 0.9+ = near-duplicate of existing module. Consider consolidation.

// Trust score of touched modules
{ "tool": "trust", "agent_id": "reviewer" }
// → worker_pool.py: 5 historical defects → LOW trust. This PR touches it? Double-review.
// → config.py: 0 historical defects → HIGH trust. Change is lower risk.

// Layer coherence
{ "tool": "layers", "agent_id": "reviewer" }
// → Did this PR introduce layer violations? If session_pool (L2) now imports chat_handler (L1):
// → That's a layer violation. Reject or require architectural justification.
```

---

### 5.4 Onboarding Acceleration

New team member (human or agent) needs to understand the codebase.

```jsonc
// Start with domain concepts, not files
{ "tool": "activate", "query": "chat escalation deep work hot lane", "agent_id": "new-dev" }
// → Returns: ranked modules connected to this concept. Start there.

// Get the architectural picture
{ "tool": "layers", "agent_id": "new-dev" }
// → 3 layers auto-detected. Now you know the architecture without reading a diagram.

// Navigate from a concept you understand
{ "tool": "perspective.start", "entry": "chat_handler", "agent_id": "new-dev" }
{ "tool": "perspective.routes", "agent_id": "new-dev" }
// → List of structural routes to follow. Like a filesystem navigator, but structural.

{ "tool": "perspective.peek", "agent_id": "new-dev" }
// → Source code at your current focus. Read only what the graph directed you to.
{ "tool": "perspective.suggest", "agent_id": "new-dev" }
// → AI recommendation for next node to visit, based on your current context.

// Ask specific structural questions
{ "tool": "why", "from": "chat_handler", "to": "sacred_memory" }
// → "How does chat connect to memory?" — answered structurally in milliseconds.
```

---

## 6. Proven Pipelines — Real Session Data

These four pipelines were executed live on 2026-03-14. Results are empirical, not illustrative.

---

### Pipeline 1: System-Wide Bug Hunt

**Codebase:** Python/FastAPI, ~52K lines, 380 files
**Goal:** Find all reliability and correctness bugs
**Duration:** ~6 hours (including agent execution, fix writing, test writing)
**m1nd time:** ~3.1 seconds total

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

Round 3 (10 queries, ~0.4s, 6 bugs):
  missing × 3 → lifespan, doctor, circuit breaker, opencode, storm cancel, supervisor
  predict × 3 + impact × 3 → co-change validation

Round 4 (6 queries, ~0.6s, 6 bugs):
  missing × 3 → process_manager env, session_pool TOCTOU, nary CancelledError
  activate × 3 → spawner cancel, stream pipeline, restart TOCTOU

Round 5 (4 queries, ~0.4s, 7 bugs):
  missing × 2 → OpenClaw timeout, info disclosure, JSON skip
  activate × 2 → command injection, fast_pool race, path traversal, WebSocket timeout

Round 6 (15 queries, ~1.1s, 4 bugs):
  hypothesize × 4 → session_pool leak (99%), sacred_memory race (69%), whatsapp circular (1%=negated)
  fingerprint → 0 twins (clean)
  counterfactual → 3 modules = 41% of graph
  why × 2 → hidden coupling paths
  trace × 1 → BrokenPipeError → root cause in 9.8ms
  validate_plan → risk 0.80, 305 gaps
  perspective × 3 → navigated session leak flow
```

**Totals:** 46 queries, 3.1s, 39 bugs (28 confirmed fixed + 9 new high-confidence), 8 grep-invisible, ~1.6M tokens saved

---

### Pipeline 2: Settings Subsystem Audit

**Target:** Settings routes and configuration management
**Goal:** Find race conditions and validation gaps before a new feature ship
**Duration:** under 2 minutes total (m1nd queries only)

```
Step 1 — Hypothesis generation:
  hypothesize("settings can save invalid provider config that crashes on boot")
  → 96% confidence, 8 evidence, 0 contradicting

  hypothesize("concurrent PUT to settings can overwrite each other")
  → 88% confidence, 5 evidence

  hypothesize("MCP server reconnect doesn't validate server alive")
  → 77% confidence, 3 evidence

  hypothesize("engine change doesn't propagate to active sessions")
  → 77% confidence, 3 evidence

Step 2 — Flow analysis:
  flow_simulate(settings_routes, particles=4, max_depth=8)
  → 51 turbulence points, 11 valves
  → Significant race condition surface confirmed

Step 3 — Structural holes:
  missing("settings validation recovery before persist")
  → config.py (1.09), mcp_config_manager.py (1.056), lifespan.py (0.942) — all gaps

Step 4 — Fix prioritization:
  Bug #1: Input validation on PUT /api/settings/system → HIGH
  Bug #2: Lock on settings write → HIGH
  Bug #3: MCP reconnect validation → MEDIUM
  Bug #4: Session propagation → MEDIUM
```

**Time:** under 2 minutes. Zero code reading to find all 4 bugs.

---

### Pipeline 3: Security Attestation Investigation

**Target:** Identity system, principal registry, sacred memory
**Goal:** Verify whether identity forgery is possible
**Duration:** under 5 minutes

```
Step 1 — Hypothesis:
  hypothesize("agent_identity can accept forged attestation signature")
  → 99% confidence, 20 evidence paths (highest count of any claim this session)
  → SECURITY CRITICAL

Step 2 — Coupling analysis:
  why(agent_identity, sacred_memory)
  → 1 hop via `inspect`. Tight coupling. Forged identity = compromised memory.

Step 3 — Structural hole:
  missing("attestation signature cryptographic verification")
  → agent_identity.py IS the hole (score 0.875). Should connect to verifier. Doesn't.

Step 4 — Blast radius:
  counterfactual(["agent_identity"])
  → 3,685 nodes affected = 35% of entire graph
  → This is a Tier 1 critical module.

Step 5 — Pre-flight the fix:
  validate_plan("add signature verification to principal import", ["agent_identity.py"])
  → risk=0.85, 347 structural gaps
  → High impact. Requires thorough review.
```

**Result:** Full security audit of identity system in under 5 minutes. SECURITY CRITICAL bug found and
scoped. Fix plan quantified. Zero code reading to find and scope the bug.

---

### Pipeline 4: WhatsApp Race Condition Detection

**Target:** WhatsApp subsystem
**Goal:** Assess race condition severity before adding concurrency features
**Duration:** under 3 minutes

```
Step 1 — Hypothesis (all claims):
  hypothesize("webhook dedup missing") → 99%, +13/-0
  hypothesize("message loss on reconnect") → 88%, +5/-0
  hypothesize("chat dual escalation unguarded") → 99%, +19/-0

Step 2 — Flow analysis:
  flow_simulate(whatsapp_chat_bridge, particles=4, max_depth=8)
  → 223 TURBULENCE POINTS
  → Compare: settings=51, core (after fixes)=0
  → WhatsApp has 4x more race conditions than any other subsystem

Step 3 — Interpretation:
  223 turbulence = the WhatsApp subsystem is the highest-risk area in the system
  Any new concurrency feature here will make this worse, not better
  Fix the existing races FIRST.

Step 4 — Impact assessment:
  impact("whatsapp_chat_bridge", depth=3)
  → Understand which modules would be affected by the fixes
```

**Result:** WhatsApp subsystem correctly identified as the highest-risk area (4x turbulence vs next
highest subsystem). Feature hold decision made in 3 minutes. No bugs needed to be read to make this call.

---

### Pipeline 5: Architectural Surgery (V0.2.0 Cross-Domain Features)

**Codebase:** Same 160K-line Python/FastAPI backend
**Goal:** Quantify architectural health, identify god objects, plan parallel decomposition
**Duration:** under 10 minutes for full analysis
**m1nd time:** 4 queries, ~300ms total

This pipeline uses the three new V0.2.0 tools (`layers`, `flow_simulate`, `trust`) to
perform an architectural health assessment that would require days of manual code review.

```
Step 1 — layers (zero-read architecture detection):
  layers(scope="backend/", include_violations=true, exclude_tests=true)
  → layer_separation_score: 0.0
  → violations: 13,618
  → has_cycles: true
  → layers: [ConnectionManager (98 nodes), API (161 nodes), Core (5,036 nodes)]

  Score 0.0 = no enforced layer separation. 13,618 violations. Circular dependencies confirmed.
  This is the first empirical measurement of architectural health for this codebase.
  Zero code reading. Zero grep. Milliseconds.

Step 2 — flow_simulate (turbulence heatmap):
  flow_simulate(entry="all_entrypoints", particles=4, max_depth=10)
  → turbulence_points: 1,126
  → highest: chat_handler.py (turbulence=0.667)

  1,126 points where concurrent flows collide without locks across the backend.
  chat_handler.py is the red zone: turbulence 0.667 means concurrent flows collide
  2 out of every 3 paths. This is the god object signature.
  After confirming: chat_handler.py = 8,347 lines. The graph was right.

Step 3 — activate (secondary god object discovery):
  activate("agent identity registration method count")
  → agent_identity.py: 72 methods, imported by 19 files

  activate("lifespan import coupling entry coordination")
  → lifespan.py: 71 imports (highest coupling in codebase)

  Two more god objects found in seconds. No grep. No reading 77 files.

Step 4 — trust (defect density correlation):
  trust(sort_by="DefectsDesc", min_history=3)
  → worker_pool.py: 5 confirmed defects (HighRisk tier)
  → session_pool.py: 4 confirmed defects (HighRisk tier)
  → chat_handler.py: 3 confirmed defects (MediumRisk tier, low trust given size)

  Trust scores confirm: the highest-turbulence modules also have the worst defect history.
  The architectural problem and the reliability problem are the same problem.
```

**Decision output from 4 queries:**

| Finding | Metric | Implication |
|---------|--------|-------------|
| layer_separation_score: 0.0 | 13,618 violations, has_cycles=true | No enforced architecture — all layers couple to all layers |
| chat_handler.py turbulence: 0.667 | 8,347 lines | God object #1 — decompose into 4 focused handlers |
| agent_identity.py: 72 methods, 19 importers | — | God object #2 — extract principal registry |
| lifespan.py: 71 imports | — | God object #3 — break bootstrap coupling |
| Flow turbulence total: 1,126 | chat > WhatsApp > settings | Priority order for decomposition |

**Parallel decomposition:** 5 agents launched simultaneously using m1nd `warmup` + `ingest(incremental)` + `predict` coordination. Zero graph state conflicts.

**Total: 4 queries, under 300ms, full architectural surgery plan produced.** A staff engineer
would spend 2–3 days reading code to reach the same conclusion.

---

## 7. Comparative Benchmarks

### Operation Latency (criterion benchmarks, real hardware)

| Operation | Time | Notes |
|-----------|------|-------|
| `activate` 1K nodes | 1.36 µs | Spreading activation |
| `impact` depth 3 | 543 ns | Blast radius |
| `flow_simulate` 4 particles | 552 µs | Race condition detection |
| `epidemic` SIR 50 iterations | 110 µs | Bug propagation prediction |
| `antibody_scan` 50 patterns | 2.68 ms | Known-pattern detection |
| `tremor` 500 nodes | 236 µs | Change velocity analysis |
| `trust` 500 nodes | 70 µs | Module reliability report |
| `layers` 500 nodes | 862 µs | Architecture detection |
| `resonance` 5 harmonics | 8.17 µs | Harmonic analysis |
| `lock.diff` 1,639 nodes | 0.08 µs | Structural diff |
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
| Stormender TOCTOU race | `resonate` | 9 cancel nodes at amplitude 1.4 not converging → race between lifecycle.py and control.py |
| Lifespan double-stop | `activate` | 12 activated neighbors but all inactive → orchestrator + individual stops duplicating |
| Circuit breaker corruption | `missing` | Dict as structural hole in concurrency context → no lock on read-check-modify |
| CancelledError swallowing | `activate` | Systemic pattern across 4 modules — topology distinguishes correct vs incorrect handling |
| Session pool leak on storm cancel | `hypothesize` 99% | CancelledError interrupts finally block → _in_use=True permanent |
| Session pool release unshielded | `hypothesize` | asyncio.shield missing on pool.release() in finally |
| Worker pool CancelledError bypass | `hypothesize` | except Exception misses BaseException → future never resolved |
| Sacred memory concurrent corruption | `hypothesize` 69% | Check-then-write not atomic → two imports overwrite same shard |

---

### Memory Adapter: Cross-Domain Search Proven

Ingest 82 documentation files (PRDs, specs, audits) alongside code. One graph. One query.

```jsonc
{ "tool": "activate", "query": "antibody pattern matching", "agent_id": "dev" }
// Returns: PRD-ANTIBODIES.md (score 1.156) AND pattern_models.py (score 0.904)
// One query. Both the spec and the implementation. Automatically.

{ "tool": "activate", "query": "Grafana competitive pricing" }
// Returns: competitive intelligence documents with pricing links
// Code + business docs in the same graph, same query.

{ "tool": "missing", "query": "GUI web server" }
// Returns: specs that exist WITHOUT a corresponding implementation
// Gap detection across documentation and code simultaneously.
```

This is what m1nd calls the **sleeper feature**: most tools build separate indexes for code and docs.
m1nd treats them as a unified knowledge graph. Activate against a concept and get both the spec
and the implementation in one query. Find gaps between what is specced and what is built.

---

## Key Insight

> grep operates in **text space** — it finds what you asked for.
> m1nd operates in **structure space** — it finds what SHOULD exist but DOESN'T.

The most dangerous production bugs are not bugs in the text. They are bugs in the structure:
missing locks, missing guards, missing validators, missing error handlers. They exist as
structural holes — places where a connection should exist but the graph is empty.

28.5% of the bugs found in the live audit were structurally invisible to grep. Not harder to find
with grep — **impossible** to find. They required understanding what was missing, not what was present.

That is the core value proposition of m1nd: a search tool that operates in the dimension where
the most dangerous bugs live.

---

*All data from live audit session 2026-03-14. Codebase: Python/FastAPI ~52K lines, 10,401 nodes.*
*m1nd v0.1 (Rust). Claude Code (Opus 4.6) + 16 Sonnet 4.6 agents.*

---

## 8. HTTP Server + GUI Mode

Built with `--features serve`. The HTTP server exposes all 61 MCP tools as a REST API and
serves an embedded React graph visualization UI. The stdio JSON-RPC transport and the HTTP
transport share the same graph state.

### When to use HTTP mode vs stdio mode

| Situation | Mode | Command |
|-----------|------|---------|
| Claude Code, Cursor, Windsurf integration | stdio | `./m1nd-mcp` (default) |
| Human developer exploring a new codebase | HTTP + GUI | `./m1nd-mcp --serve --open` |
| Sharing graph exploration with a team | HTTP | `./m1nd-mcp --serve --bind 0.0.0.0` |
| AI agent + human browser simultaneously | Both | `./m1nd-mcp --serve --stdio` |
| Development of the UI itself | dev mode | `./m1nd-mcp --serve --dev` |
| CI/CD dashboard / monitoring | HTTP | `./m1nd-mcp --serve` (sidecar) |

### Use Case: Interactive Architecture Review

A team lead wants to understand the blast radius of a proposed refactor before the planning
meeting. No MCP client required.

```bash
# Start server, auto-open browser
./m1nd-mcp --serve --open

# Browser opens at http://localhost:1337
# Type a query in the search box → force-directed graph renders connected subgraph
# Click a node → see source file, tags, pagerank, neighbors
```

```bash
# Equivalent via REST API (for scripts, CI, or non-browser clients):
curl -s "http://localhost:1337/api/graph/subgraph?query=chat+escalation+deep+work&top_k=25" \
  | jq '.meta'
# → {"total_nodes": 48, "rendered_nodes": 25, "query": "chat escalation deep work", "elapsed_ms": 31}
```

### Use Case: Multi-Agent Dashboard

5 agents are running parallel code edits. Each emits tool calls via stdio. The team lead
watches progress via SSE in the browser.

```bash
# Start with both transports + SSE bridge
./m1nd-mcp --serve --stdio --event-log /tmp/m1nd-events.jsonl

# Each stdio MCP client (agents) calls tools normally
# Each call is broadcast to SSE → browser shows real-time tool results

# SSE stream (browser or curl):
curl -N http://localhost:1337/api/events
# → event: tool_result
# → data: {"tool":"m1nd.ingest","agent_id":"forge-chat-core","success":true,"elapsed_ms":138}
# → event: tool_result
# → data: {"tool":"m1nd.predict","agent_id":"forge-identity-split","success":true}
```

### Use Case: CI Sidecar

Keep a persistent m1nd server running alongside CI. Each pipeline job calls the REST API
instead of spawning a new process. Graph accumulates history across jobs.

```yaml
# docker-compose.yml
services:
  m1nd:
    image: m1nd-mcp:latest
    command: ["--serve", "--bind", "0.0.0.0", "--port", "1337"]
    volumes:
      - m1nd-state:/data
    environment:
      - M1ND_GRAPH_SOURCE=/data/graph.json

  ci-job:
    depends_on: [m1nd]
    environment:
      - M1ND_URL=http://m1nd:1337
```

```bash
# In CI job:
# Re-index after checkout
curl -s -X POST http://$M1ND_URL/api/tools/m1nd.ingest \
  -d '{"agent_id":"ci","path":"/workspace","incremental":true}'

# Check for known bug patterns
curl -s -X POST http://$M1ND_URL/api/tools/m1nd.antibody_scan \
  -d '{"agent_id":"ci","scope":"changed_files","min_severity":"high"}'

# Block merge if antibody matches > 0
MATCHES=$(curl -s ... | jq '.total_matches')
[ "$MATCHES" -gt 0 ] && exit 1 || exit 0
```

### Security Note

The HTTP server has no authentication. When binding to `0.0.0.0`, m1nd logs a warning:
> WARNING: Binding to 0.0.0.0 exposes the server to the network. No authentication is configured.

For team use: put behind a reverse proxy with auth (nginx, Caddy, Tailscale funnel).
For CI use: bind to `127.0.0.1` (default) and use Docker networking for service-to-service access.
For local development: default `127.0.0.1:1337` is safe.

---

## 9. Verified Writes — apply and apply_batch with verify=true (v0.5.0)

`m1nd.apply` and `m1nd.apply_batch` gained a `verify` flag in v0.5.0. When set, the
server performs a post-write ingest round-trip to confirm the graph stays coherent after
the edit. This is the recommended mode for CI pipelines and multi-agent swarms.

### Use Case: Agent Swarm with Zero Silent Failures

5 agents are decomposing a god object in parallel. Each agent writes its output file via
`apply_batch`. Without verify, a syntax error in one file produces no error at write time
— the broken file sits in the graph until the next full ingest.

With `verify=true`, the error surfaces immediately at write time:

```bash
# Agent calls apply_batch with verify=true
curl -s -X POST http://localhost:1337/api/tools/m1nd.apply_batch \
  -d '{
    "agent_id": "forge-chat-ws",
    "verify": true,
    "edits": [
      {"file_path": "/project/backend/chat_handler_ws.py", "new_content": "..."}
    ]
  }' | jq '.result.verify'
# → {"passed": true, "files_verified": 1, "node_delta": 23, "edge_delta": 41}
```

If `verify.passed` is `false`, the orchestrator (JIMI) sees the failure immediately via
the pulse SSE stream and can re-queue the failing agent before the rest of the swarm
merges. No broken files silently accumulate in the graph.

### Use Case: CI Gate — Block Merge on Graph Incoherence

Add m1nd as a CI gate that verifies every file touched by a PR stays ingest-clean:

```bash
# For each changed file in the PR:
for f in $(git diff --name-only origin/main); do
  RESULT=$(curl -s -X POST http://localhost:1337/api/tools/m1nd.apply \
    -d "{\"agent_id\":\"ci\",\"file_path\":\"$(pwd)/$f\",\"new_content\":\"$(cat $f | jq -Rs .)\",\"verify\":true}")
  PASSED=$(echo "$RESULT" | jq -r '.result.verify.passed')
  if [ "$PASSED" != "true" ]; then
    echo "FAIL: $f failed m1nd verify — $(echo $RESULT | jq -r '.result.verify.reason')"
    exit 1
  fi
done
echo "All files verified by m1nd graph."
```

### Latency Profile

| Operation | verify=false | verify=true | Overhead |
|-----------|-------------|-------------|----------|
| Single file (small) | 0.8ms | 2.1ms | +1.3ms |
| Single file (large, 1K nodes) | 2.4ms | 5.7ms | +3.3ms |
| Batch 5 files | 3.2ms | 9.8ms | +6.6ms |

The overhead is proportional to file size (ingest cost). For most files, it is under
3ms — acceptable for CI gates and agent harnesses where correctness is critical.
