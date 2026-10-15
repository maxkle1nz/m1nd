# First simplification slice: source retrieval without execution proxies

Status: **Unreleased source candidate, 2026-10-15.** This document describes the
current branch. It does not alter the sealed 1.6.5 release or its installation guide.

## Withdrawn capabilities

- `transplant`, `transplant_preview`, and `transplant_commit`: graph-authored Rust
  symbol moves, their preview handles, and their exclusive algorithm tests.
- `mission_spawn`, `candidate_naming`, and `curation_spawn`: owner-to-runner execution,
  automatic candidate naming, and automatic curation proposal application.
- The `m1nd-runnerd` workspace member, its runner registry/status/announcement
  surfaces, UI runner polling, and launch controls.

These names are removed from active schemas, help discovery and dispatch. Requests
using an old name do not launch a process or write source. Compatibility entries in
the frozen action catalog remain as tombstones: an old catalog identifier is not a
callable endpoint, authority grant, or future commitment to restore the feature.

Candidate scans use provisional heuristic names. Historical `named_by:"runner"`
provenance remains readable; it does not announce or start a runner. The existing
`naming:"auto"` input spelling may remain as a compatibility alias for heuristic
naming, with no runner call. Review names manually through `candidate_edit`.

## Retained workflow and boundaries

The agent still binds an explicit canonical workspace, prepares or reuses private
derived state, and follows `north → seek/search → view/surgical_context → impact/why`.
Source anchors, scope checks, honest omissions/staleness, and impact analysis remain.
The host agent performs edits and runs commands with its existing tools.

This slice retains other source writers (`apply`, `apply_batch`, `edit_preview`,
`edit_commit` and governed mutation paths), manual missions, mission letters,
`candidate_edit`, candidate leases, human ratification/receipt landing, promotion,
HTTP, the UI and the OpenClaw bridge. Their authority floors and safety oracles
remain required. Clipboard mode writes no engine state. Manual/direct mode posts a
mission letter and copies a packet for the human to deliver; it does not launch an
agent or apply a proposal. Manual curation uses that same letter/clipboard path.

The default menu is still derived from the core plus host-binding tools. `help`
continues to discover the full **retained** registry; withdrawn names are absent.
Hiding a verb from `tools/list` alone would not implement this withdrawal.

## Dependency and recovery constraints

- Owner pinning is a binding fact, independent of the removed naming-runner object.
  Removing the proxy must preserve foreign-root ingest/rebind refusal.
- `external_mutation_service` still governs ingest and retained writers. Its shared
  admission code cannot be removed with an execution proxy.
- Mission/control types, execution bookkeeping needed by retained services, and
  strict checkpoint sidecars remain where they have surviving consumers.
- Checkpoint/CURRENT, actor and lease identity, serialization and recovery rules
  remain strict. This slice does not default away fields, silently migrate stores,
  erase notes, rebuild user state, or relax authority on old snapshots.

The branch is reversible through ordinary Git commits. Reverting it must not
require reconstructing checkpoints or deleting runtime data.

## Proof and limits

Retire tests only with their exclusive implementation. Retain distinct tests for
installation verification, canonical workspace isolation and restart, source
retrieval/impact, source-file integrity, manual mission writes, authority refusals,
checkpoint recovery and the UI/HTTP contracts still shipped. UI fixtures must not
provide withdrawn routes as successful mock APIs. Manual clipboard, OCC conflict,
read-only refusal, old provenance and human-only landing oracles remain.

No test budget, retry count, required gate, platform or authority floor is weakened
by this contract. Expensive repeated fixtures are a separate measured question;
this slice makes no runtime, token, productivity or test-duration claim.

Validation belongs to the resulting exact commit. A local macOS result cannot
establish Windows readiness. PR #582 still has unresolved Windows fixture failures
unless a separately identified native Windows run proves a fix. Existing PRs and
the isolated dependency patch are separate candidates, not automatically integrated
proof for this branch. No merge or release follows from this document.

The transplant/F11/F12/F25 and organism PRDs, UML histories and dated benchmark
receipts remain historical records. Their runner/transplant designs are superseded
for current source by this contract; they are not current call recipes. No broader
writer, mission, promotion, UI or state-format withdrawal is included here.
