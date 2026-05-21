# Bug-Hunt Audit Lane: audit-01

Round: `bughunt-p-limit-mc1-smoke-20260518T000000Z`
Repo: `p-limit`
Instruction mode: `m1nd-mission-control`
Workspace: `/Users/kle1nz/m1nd/.m1nd-field-workspaces/bughunt-p-limit-mc1-smoke-20260518T000000Z/audit-01/p-limit`

Work as if this is a real production-minded dependency audit.
Do not guess the benchmark hypothesis, bug count, or comparison arm.
Find real behavioral defects, edge-case regressions, missing tests, or contract mismatches.
Do not patch files. Do not read `operator-only/` artifacts.

## m1nd Mission Control Mode

Use Mission Control v1 as the operating loop for this audit.
Mission Control is not a replacement for source reads, tests, compiler output, or runtime proof.

Required operating loop:

1. Establish trust with `trust_selftest`, or `session_handshake` scoped to this repo.
2. Prefer the isolated helper runtime for Mission Control benchmark calls, even when native host tools are visible: `probe_m1nd.py --binary /Users/kle1nz/m1nd/target/debug/m1nd-mcp --no-worktree-artifacts --runtime-dir /Users/kle1nz/m1nd/docs/benchmarks/bug-hunt-rounds/bughunt-p-limit-mc1-smoke-20260518T000000Z/m1nd-runtime/audit-01 --workspace-root <repo> call <tool> <json>`. This keeps mission state scoped to this lane and avoids a host graph that may contain benchmark operator-only artifacts.
3. Use the native host MCP surface only if `trust_selftest` or `session_handshake` proves the active workspace binding is exactly this lane workspace and the host exposes `mission_start`, `mission_event`, `mission_next`, `mission_verify`, `mission_handoff`, and `mission_close`. Record `mission_transport="native_host_mcp"` when you do.
4. Record `mission_control_unavailable=true` only when neither the native host surface nor the helper surface can call Mission Control. Then fall back to the `m1nd-trained` loop and do not fake mission calls.
5. Start a repo-scoped mission with `mission_start`: `agent_id=<lane_id>`, `repo=<workspace>`, `task="bug-hunt audit for behavioral defects"`, `mode="bug_hunt"`, `budget="normal"`, and `risk="medium"`.
6. Take the starter move, record meaningful actions with `mission_event`, then call `mission_next` after meaningful events.
7. Treat `do_not` entries from `mission_next` as guardrails. If you disagree, record a dissent event explaining the chosen tool and required evidence.
8. When `mission_next` switches to direct proof, stop graph exploration and use direct source reads, rg, tests, compiler output, or focused runtime probes.
9. Call `mission_verify` before finalizing material findings. Reference direct evidence explicitly, such as `event:evt_1`, `file_read:path:line`, `test_run:name`, or `runtime_probe:id`; an unrelated direct event must not validate a graph-only claim.
10. In `bug_hunt`, if `mission_next` returns `move.type="direct_sweep"`, do the requested negative-space sweep before closing: public contracts/docs, boundary values, error paths, async/concurrency behavior, and helper/exported APIs. Record it as `coverage_sweep`, `boundary_sweep`, or `edge_case_sweep`.
11. Call `mission_handoff` before final result writing so the lane has a resumable packet.
12. Call `mission_close` before writing the final lane JSON; preserve gaps, non-claims, event digest, and proof-packet summary.
13. Fill `mission_control_usage` in the lane result with `mission_id`, route, transport, call counts, unavailable state, `do_not` guardrails, verified/rejected claims, direct-proof switches, coverage sweeps, event digest, handoff summary, and proof-packet summary.
14. Also preserve raw m1nd calls in `m1nd_usage` when useful for auditability.

## Required Output

Write your final JSON result to `/Users/kle1nz/m1nd/docs/benchmarks/bug-hunt-rounds/bughunt-p-limit-mc1-smoke-20260518T000000Z/lane-results/audit-01.json`.
Append investigation events to `/Users/kle1nz/m1nd/docs/benchmarks/bug-hunt-rounds/bughunt-p-limit-mc1-smoke-20260518T000000Z/event-streams/audit-01.jsonl` using `event_source="agent"`.
Every event must include `schema`, `round_id`, `lane_id`, `event_source`, `event_type`, and `created_at`.
Record at least `audit_started`, one first-discovery event such as `findings_identified`, `focused_probes`, or `runtime_probe`, and `result_written`.
Use ISO timestamps; do not use `ts` or `event` as substitutes in new rounds.
Use the schema in `lane-result-template.json`.

Findings should include title, severity, file, symbol, cause, impact, evidence, reproduction_or_test, and confidence.
Extra findings are welcome, but they must be concrete and source-backed.
