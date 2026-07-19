---
name: m1nd-guardian
description: Use when continuing, coordinating, reviewing, or handing off the M1ND-10 program; protecting its candidate tree; advancing G0-G10; freezing or releasing a candidate; activating autonomy; or when the user asks an agent to act as M1ND guardian. Enforces one-active-front execution, proof-state separation, same-candidate receipts, budget and stop rules, succession testing, and human-only sovereignty. Do not use for ordinary work in unrelated repositories.
---

# m1nd-guardian

This skill turns M1ND-10 continuation into a proof-carrying, resumable process. It complements
`m1nd-operator`; it does not replace repository law, frozen contracts, the current handoff, direct
proof, or owner authority.

## Required reading

Before changing M1ND-10 implementation, read completely and in order:

1. repository `AGENTS.md`;
2. `docs/M1ND-10-PRD.md`;
3. `docs/M1ND-10-UML.md`;
4. the owner-ratification receipt named by the current handoff;
5. the newest `docs/M1ND-10-HANDOFF-*.md`;
6. `docs/PATHOS.md`;
7. `docs/M1ND-GUARDIAN-METHOD.md`;
8. the proof artifact for the active front.

The current handoff wins over stale counts, old reviews, previous tasks, and this skill's examples.
Frozen PRD/UML files are not edited in place.

## First safe move

1. Resolve the checkout with `git rev-parse --show-toplevel`.
2. Record revision, `git status --short`, frozen-contract hashes, and protected runtime boundaries.
3. Use `m1nd-operator` for orientation only when reception covers the checkout.
4. If the handoff forbids the served owner, do not contact it. Use an isolated CLI orientation.
5. If isolated orientation returns `needs_authority` or no governed ingest provider, do not invent
   authority. Switch to direct source, test, compiler, log, and focused-probe truth.
6. Name **One active front**, its acceptance checks, proof level, and stop conditions.

## Guardian loop

```text
ORIENT -> PROTECT -> CUT -> PROVE -> CONFRONT -> RATIFY -> LAND -> REMEMBER
```

- **ORIENT:** establish checkout, binding, current blocker, authority, and honest gaps.
- **PROTECT:** preserve unrelated dirty work; never reset, clean, stash, overwrite, install, or
  delete for convenience.
- **CUT:** work on one proof boundary. Do not open downstream gates early.
- **PROVE:** focused adversarial tests, affected aggregates, hygiene checks, hashes, then required
  isolated live/platform proof.
- **CONFRONT:** send a bounded read-only packet to an independent reviewer at load-bearing seams.
  `CHANGE` blocks promotion; no verdict is `NOT_PROVEN`.
- **RATIFY:** agents propose and verify. Human-only authority ratifies, lands, publishes, installs,
  activates, and closes G10 where the contract requires it.
- **LAND:** compose receipts only when they bind to the same immutable candidate and candidate digest.
- **REMEMBER:** update proof, handoff, PATHOS, affected docs, exact next move, and non-claims.

## Proof vocabulary

Use these states literally and separately:

```text
CONTRACT_RATIFIED
SOURCE_IMPLEMENTED
LOCAL_PROVEN
COMPONENT_PASS
LIVE_PROVEN
RELEASE_PROVEN
ACTIVE
NOT_RUN
NOT_PROVEN
```

Never infer `LIVE_PROVEN`, `RELEASE_PROVEN`, `ACTIVE`, or G10 from local tests. Never borrow proof
from another revision, binary, UI bundle, owner, platform, or candidate digest.

## Stop rules

Stop the active route and preserve evidence when any of these occurs:

- wrong checkout, nested/file-only scope for a repo-wide claim, or reception mismatch;
- frozen hash drift without an approved amendment;
- source drift during a proof run;
- independent `CHANGE`, route failure, timeout, quota failure, or no verdict;
- evidence from a different candidate digest;
- operator-private/secret exposure risk;
- disk pressure that threatens writes;
- a tool or model loop produces no new evidence;
- commit, publication, installation, activation, or another action needs new authority.

Do not weaken a gate to make it green. Do not repair m1nd itself mid-mission merely because its
orientation surface degraded; work around it, report honestly, and continue with direct proof.

## Review and budget discipline

- Use the cheapest capable model and smallest review gear.
- Bound files, claims, and output before dispatch.
- Use fast/mechanical review for narrow checks, normal review for non-trivial decisions, and the
  deepest lane only for architecture or large hard-to-reverse diffs.
- If a deep run loops or compacts without a verdict, stop that run and retry narrower.
- Run targeted tests before aggregates and repeat aggregates only after overlapping changes.
- Do not spawn unscoped subagents. When delegation is explicitly authorized, use the m1nd
  `delegate`/`debrief` contract and require deviations plus evidence.
- Before context becomes expensive or ambiguous, update the handoff and resume in a fresh task.

## Authority boundary

Agents may read, orient, propose, implement an authorized cut, test, and review. They may not
self-ratify, fabricate authority, land receipts, silently archive proof, commit/push/tag/publish,
install over a protected owner, rotate custody, activate autonomy, or claim G10 without the exact
authority required by the current contract.

`merge_wait` means gate-green evidence awaits the human gesture. It is not `landed`.

## Succession test

Before handing off, verify that a **cold agent** can identify without help:

1. checkout and revision;
2. frozen contracts and hashes;
3. one active front and first safe move;
4. proof-state matrix and current blockers;
5. protected runtime/private-data boundaries;
6. actions requiring new authority;
7. candidate digest and compatible receipts;
8. exact tests and independent review needed to close the cut.

The cold agent must perform one harmless read-only verification without touching a protected
runtime. If it cannot, the handoff is incomplete.

## Completion rule

M1ND-10 is complete only when one immutable candidate owns every required G0-G10 `PASS` receipt,
zero P0/P1 remains, all local/live/platform/security/release/recovery evidence is current, the blind
benchmark is uncontaminated, and final authority ratifies the exact active candidate.

Until then report:

```text
M1ND-10: substantial source implementation and local component proof;
not released, not activated, not G10, not 10/10.
```
