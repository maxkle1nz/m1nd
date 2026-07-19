# M1ND-10 Typed Elevated Consumer Preflight

Date: 2026-07-18  
Observation point: dirty working tree at `b59a1c2a1454a83164dfb4d5640c6b005154d1ee`  
Status: **architecture APPROVED; implementation and production adoption NOT PROVEN**

## Decision boundary

The generic MCP/REST dispatcher currently admits only actions whose exact
authority floor is `Ordinary`. `ScopedGrantA2`, `PositiveSovereign`,
`ServiceIdentity`, and `SafetyOnly` actions fail closed before their legacy
handlers can produce an effect. This is a valid security boundary, but it is
also an availability and M1ND-10 completeness boundary: outside the typed
`MissionService`, elevated public workflows are securely unavailable.

The frozen contracts remain unchanged:

- `docs/M1ND-10-PRD.md` — `00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38`
- `docs/M1ND-10-UML.md` — `8a8a5fe9b9d2a4fc62c419e160e8dc2dcb4115f58d98f3f15a2d5031881dd32b`

## Approved implementation law

1. Preserve the generic `Ordinary`-only wall. No elevated action may be
   re-enabled by a caller-provided label, identity, floor, lease header, or
   legacy handler predicate.
2. Add closed typed request envelopes whose semantic action and canonical
   object/payload digest are derived by the owner.
3. Resolve authority only through the owner broker: verify the signed receipt,
   exact ingress/brain/session/mission/head/version/effects bindings, current
   epochs/freeze/grant state, and reserve the one-shot lease before dispatch.
4. Reuse an existing mutation handler only behind a typed transactional adapter.
   No effect may become visible before the corresponding commit boundary.
5. Keep the consumer families distinct: A2 source/graph/candidate operations,
   positive-sovereign governance/release operations, pinned service-identity
   operations, and the physically separate negative-only safety actuator.
6. Generate or validate an exhaustive `action catalog x ingress x consumer`
   matrix. Every reachable elevated tuple must name a typed consumer or project
   an explicit `policy_disabled`; absence is a mechanical failure.
7. Begin with a vertical slice covering ratify, promotion, and one A2 edit, then
   expand without weakening the same contract.

## Independent read-only verdict

The proposal and its source dossier were submitted to askGOD/Fugu before any
typed-consumer implementation.

```text
VERDICT: APPROVE
CONFIDENCE: alta
REQUIRED_CHANGES:
1. NONE
```

The reviewer found the proposal to be the smallest safe architecture because
it follows the existing `MissionService` pattern, preserves server-derived
action/digest classification, consumes authority through the owner broker, and
keeps positive, service, and safety effects separated. Approval is conditional
on treating handler reuse as an adapter behind the transaction boundary, never
as direct early mutation.

The reviewer also identified three proof obligations that the implementation
must not omit:

- non-MCP parity for CLI, hooks, jobs, recovery, migrations, and advertised
  executable surfaces, or explicit `policy_disabled` projection;
- target digest, proof mark, OCC, rollback/conservation, and crash semantics for
  A2 source filesystem edits;
- same-UID tamper and rollback resistance for every new sidecar store, separate
  from the broker/WAL happy path.

## Current truth

**DONE / PROVEN in the preceding slice**

- exact action catalog and fail-closed routing;
- generic elevated refusal with zero handler side effect;
- authority session, signed receipt, lease reservation, replay refusal, and
  typed `MissionService` consumption;
- `MissionService.land` integration through an exact verified WAL COMMIT
  witness.

**NOT PROVEN by this preflight**

- implementation of the new elevated consumers;
- positive ratify/promote/A2 workflows over public ingress;
- complete consumer parity across every ingress;
- source-write rollback, same-UID race resistance, or safety actuator behavior;
- production hardware authority, activation, hosted release, `G10`, or
  `FULL_AUTONOMY`.

This receipt approves an implementation direction. It is not a gate receipt,
an activation receipt, or release authority.
