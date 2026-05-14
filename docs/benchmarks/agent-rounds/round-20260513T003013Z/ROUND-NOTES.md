# Agent Reliability Round Notes

Round: `round-20260513T003013Z`
Condition: `host-recovery`
Status: internal evidence, not public performance copy

## Result

This round is structurally complete: three `m1nd_available` lanes, three
`no_m1nd` lanes, and one adjudication lane produced lane-result JSON.

It is not a full public comparison because live-required tasks were not equally
proved across arms:

- `structurally_comparable_primary_arms=true`
- `live_proof_comparable_primary_arms=false`
- `comparable_primary_arms=false`
- `public_claim_worthy=false`

## Main Finding

The control lanes were strong at reading docs and source routes. They could
often explain the correct recovery path from files alone.

The m1nd lanes were stronger at exposing live operational state: workspace
binding, host/runtime mismatch, blocked retrieval recovery, and whether a live
proof was actually missing.

That distinction is the product lesson. For agent-first reliability, "knows the
route" and "proved the current session state" must be scored separately.

## Metrics Snapshot

- `m1nd_available`
  - success rate: `0.7143`
  - median run score: `121`
  - live-required verified rate: `0.5833`
  - live proof gaps: `5`
- `no_m1nd`
  - success rate: `0.9048`
  - median run score: `127`
  - live-required verified rate: `0`
  - live proof gaps: `12`

The raw success score favors controls, but the adjudication shows why that is
not a safe product claim: static route knowledge was over-scored on tasks that
needed live host/runtime/session evidence.

## Product Patch Queue

1. Require `proof_mode`, `live_state_verified`, `evidence_origin`, and
   `raw_event_evidence` in future live-proof rounds.
2. Split structural comparability from live-proof comparability.
3. Cap or flag any `requires_live_proof=true` task scored as success or high
   proof without `live_state_verified=true`.
4. Add a future runner that injects real `Transport closed`, wrong-workspace,
   stale-runtime, and continuity-trail conditions instead of asking agents to
   infer them from docs.
5. Preserve raw event streams so adjudication does not depend only on compact
   lane JSON summaries.

## Non-Claims

- This round does not prove m1nd is faster overall.
- This round does not prove a public benchmark win.
- This round does not prove every host can recover automatically.
- This round does not prove transport recovery, because no real dead transport
  was injected in the primary lanes.
- This round does show the next benchmark contract needed for credible
  agent-first evidence.
