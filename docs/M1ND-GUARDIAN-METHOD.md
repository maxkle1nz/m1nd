# M1ND Guardian Method

Status: operational doctrine. This document is not a PRD/UML amendment, a gate receipt, or
authority to commit, publish, install, activate, ratify, or land evidence.

## 1. Purpose

The Guardian Method makes long-running M1ND development resumable by a cold agent without
weakening truth, authority, or proof boundaries. Its core loop is:

```text
ORIENT -> PROTECT -> CUT -> PROVE -> CONFRONT -> RATIFY -> LAND -> REMEMBER
```

The method exists to prevent false progress. A guardian is successful when the next safe move is
clear, the evidence level is honest, and no unearned state transition occurred.

## 2. Authority and precedence

For M1ND-10 work, read current repository truth in this order:

1. `AGENTS.md` — repository law, public no-leak rules, gates, and live-runtime restrictions.
2. `docs/M1ND-10-PRD.md` — frozen requirements and G0-G10 acceptance contract.
3. `docs/M1ND-10-UML.md` — frozen topology, state machines, and trust boundaries.
4. The current owner-ratification receipt named by the handoff.
5. The current `docs/M1ND-10-HANDOFF-*.md` — volatile implementation truth and exact next cut.
6. `docs/PATHOS.md` — organism history, doctrine, known problems, and continuity.
7. Current proof artifacts and direct source/test/runtime evidence.
8. This method — execution discipline only.

Higher items override lower items. A stale method, PATHOS paragraph, review, test count, or chat
message never overrides a newer handoff or direct proof. Frozen PRD/UML files are not edited in
place; a real contract change requires an explicit amendment and owner ratification.

## 3. The loop

### 3.1 Orient

1. Resolve the repository mechanically with `git rev-parse --show-toplevel`; never trust a
   remembered machine-local path.
2. Read the current handoff before choosing work. Resume from its proof matrix and active blocker,
   not from a generic repository recap.
3. Use `m1nd-operator` for structural orientation when a correctly bound graph is available.
4. Reception governs writes. Under `caller_root_mismatch`, retrieval is orientation-only and all
   m1nd write verbs are prohibited.
5. If isolated orientation reports `needs_authority`, `needs_ingest`, or an unavailable governed
   provider, do not invent authority or call a legacy ingest path. Switch to direct source, tests,
   compiler output, logs, and focused probes.
6. The graph narrows and connects. Direct proof decides what is true.

### 3.2 Protect

Before each implementation cut:

- capture `git status --short`, the current revision, and the hashes of every frozen contract;
- preserve unrelated modifications and untracked files;
- treat a large dirty tree as valuable until proven otherwise;
- use isolated temporary roots for generated proof state;
- never reset, clean, stash, overwrite, install, or delete material merely to simplify a gate;
- obtain explicit authority before making an external recoverable snapshot or deleting even
  regenerable large output;
- never inspect operator-private material from an implementation or reviewer role;
- never use the installed owner or its production port when the handoff excludes it.

A proof run that observes uncontrolled source drift does not bind the final tree. Repeat it on a
stable snapshot or label it historical.

### 3.3 Cut

Maintain **one active front** and one named proof boundary.

A cut must state:

```text
objective
in-scope paths
protected paths
acceptance checks
authority available
proof level sought
stop conditions
```

Do not open a downstream gate while its prerequisite is blocked. Do not fix unrelated defects in
the middle of a proof run. Record them for the next cut.

### 3.4 Prove

Use the smallest proof ladder that can honestly close the cut:

```text
adversarial focused tests
-> subsystem tests
-> affected aggregates
-> lint / format / diff / no-leak checks
-> frozen-contract hashes
-> isolated runtime or platform proof when required
```

Every report uses the literal state vocabulary:

| State | Meaning |
|---|---|
| `CONTRACT_RATIFIED` | The owner approved the governing contract. |
| `SOURCE_IMPLEMENTED` | The source exists in the stated tree. |
| `LOCAL_PROVEN` | Named deterministic local evidence passed. |
| `COMPONENT_PASS` | A component has evidence but its cumulative gate remains open. |
| `LIVE_PROVEN` | The exact candidate crossed the real runtime boundary. |
| `RELEASE_PROVEN` | The immutable candidate passed hosted release/install/rollback gates. |
| `ACTIVE` | A valid prior-authority receipt activated the exact mode or release. |
| `NOT_RUN` | The required exercise was not executed. |
| `NOT_PROVEN` | A claim lacks the evidence required by its gate. |

Local green tests never imply `LIVE_PROVEN`, `RELEASE_PROVEN`, `ACTIVE`, or G10.

### 3.5 Confront

Use an independent reviewer or oracle at load-bearing boundaries, not for routine edits.

- Use the cheapest capable review lane for the real risk.
- Bound files, claims, and questions before dispatch.
- Prefer read-only review with no unrelated MCP servers or mutation surfaces.
- Fingerprint relevant source state before and after review.
- Require a structured verdict: `APPROVE`, `CHANGE`, or `REJECT`, confidence, evidence, required
  changes, and missed risks.
- `CHANGE` blocks the boundary until every required change is implemented, gates rerun, and a fresh
  review binds the corrected diff.
- Route failure, timeout, quota exhaustion, or no verdict is `NOT_PROVEN`, never approval.
- If a heavy review loops or compacts without a verdict, stop only that run and retry with a
  narrower packet.

The reviewer must not become the evidence source for behavior it did not execute.

### 3.6 Ratify

Agents may investigate, propose, implement within authority, test, reject, and request review.
They may not self-grant sovereignty.

Human-only transitions include:

- ratifying a candidate skeleton or frozen contract;
- landing a receipt through the accepted human gesture;
- archiving a superseded receipt where the contract requires a human gesture;
- authorizing commit, push, tag, publication, installation, key/custody changes, or activation;
- issuing final G10 ratification.

Evidence is not authority. A plausible token, fixture key, declared identity, or agent consensus
does not create authority.

### 3.7 Land

All cumulative proof must bind to the **same immutable candidate**.

Do not compose evidence from different source trees, binaries, UI bundles, action catalogs,
policies, owners, environments, or candidate digests. A green gate without an imported receipt is
`merge_wait`, not `landed`.

The target autonomy posture is staged:

```text
HUMAN_GATED -> POLICY_AUTONOMOUS -> FULL_AUTONOMY
```

Agents may run the reviewer/bounce/train loop repeatedly, but `FULL_AUTONOMY` becomes active only
through the exact prior-authority activation transaction required by the frozen contract.

### 3.8 Remember

Close every meaningful cut by updating the durable continuity surfaces that the change affects:

- proof artifact;
- current handoff;
- `docs/PATHOS.md`;
- code/API/user documentation required by `AGENTS.md`;
- exact next move, blockers, commands run, and evidence level.

Never make PATHOS an optimism log. Declared tissue may carry purpose and doctrine; verifiable tissue
must remain mechanically checkable. Near a handoff, use a freshness receipt when the correctly
bound soul-check surface is available. Otherwise label freshness `NOT_PROVEN` and verify directly.

## 4. Stop rules

| Condition | Required response |
|---|---|
| Wrong repository or reception mismatch | Stop m1nd writes; resolve binding or use direct proof. |
| Governed ingest/authority unavailable | Do not synthesize it; continue only on direct read/test truth. |
| Frozen hash changed unexpectedly | Stop, preserve evidence, and request an explicit amendment decision. |
| Source changed during a proof run | Invalidate or label the run historical; rerun on a stable snapshot. |
| Independent verdict is `CHANGE` | Block promotion and close every required change before re-review. |
| No valid independent verdict | Keep the review row `NOT_PROVEN`. |
| Evidence carries another candidate digest | Refuse composition; rerun on the selected candidate. |
| Operator-private or secret material is encountered | Stop exposure, preserve the boundary, and report without copying content. |
| Disk pressure threatens writes | Stop write-heavy work; inventory first and obtain deletion authority. |
| Tool/model loop yields no new evidence | Stop the run, checkpoint, narrow the task, and hand off. |
| New authority is required | Stop and ask the owner; do not widen scope implicitly. |

## 5. Budget discipline

- Use the smallest capable model and review depth.
- Give each run one objective, one evidence boundary, and explicit stop conditions.
- Prefer targeted tests before aggregates; repeat aggregates only after overlapping source changes.
- Do not reload broad history when the handoff names exact files and proof artifacts.
- Do not use subagents for an unscoped exploration. When delegation is authorized, send a bounded
  packet and require deviations, findings, and direct evidence on return.
- A long loop without new evidence is a failed route, not persistence.
- Before context becomes unwieldy, update the handoff and continue in a fresh task.

## 6. Authority matrix

| Action | Default seat |
|---|---|
| Read, orient, inspect, and run non-mutating diagnostics | Agent |
| Edit inside an explicitly authorized cut | Implementing agent |
| Review a load-bearing diff | Independent agent/oracle, read-only by default |
| Import/land or archive governed receipts | Human gesture defined by the contract |
| Ratify frozen canon or candidate map | Human owner |
| Commit, push, tag, publish, install, rotate custody, activate | Explicit owner authority |
| Final G10 decision | Authority required by the active mode |

The stricter current handoff or action policy always wins.

## 7. Succession test

A cold agent is ready to continue only if it can answer and demonstrate all of these without help:

1. Which checkout and revision are actually under review?
2. Which documents are frozen, and do their hashes match?
3. What is the one active front and its first safe move?
4. What is implemented, locally proven, live proven, release proven, active, or not run?
5. Which runtime, private data, and paths are outside the current proof boundary?
6. Which actions require new authority?
7. Which receipts bind to the current candidate digest?
8. Which exact tests and review close the current cut?

The cold agent must then perform one harmless verification — for example, frozen-hash validation or
a focused read-only contract test — without touching a protected runtime. Failure means the handoff
is not yet sufficient.

## 8. Current M1ND-10 application

This section is orientation-only and becomes stale as soon as the handoff advances.

At the 2026-07-19 checkpoint, the handoff names candidate-source hardening as the only active front.
Candidate freeze remains blocked until the required policy/content remediation, adversarial tests,
focused gates, and a fresh independent `APPROVE` are complete. Formal blind benchmarking, live UI,
hosted release, installation, autonomy activation, and served-owner contact do not belong inside
that corrective cut.

Always re-read the current handoff before acting.

## 9. Completion

M1ND-10 is complete only when one immutable candidate carries all G0-G10 `PASS` receipts, no P0/P1
remains, every required local/live/platform/security/release/recovery gate is green, the blind
benchmark remains uncontaminated, the adversarial review is current, and the required authority
issues final ratification.

Until then, use the honest headline:

```text
M1ND-10: substantial source implementation and local component proof;
not released, not activated, not G10, not 10/10.
```
