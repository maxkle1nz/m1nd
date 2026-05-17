# Bug-Hunt Audit Lane: audit-04

Round: `bughunt-p-limit-mc0-20260517T134906Z`
Repo: `p-limit`
Instruction mode: `m1nd-mission-control`
Workspace: `/Users/kle1nz/m1nd/.m1nd-field-workspaces/bughunt-p-limit-mc0-20260517T134906Z/audit-04/p-limit`

Work as if this is a real production-minded dependency audit.
Do not guess the benchmark hypothesis, bug count, or comparison arm.
Find real behavioral defects, edge-case regressions, missing tests, or contract mismatches.
Do not patch files. Do not read `operator-only/` artifacts.

## m1nd Mission Control Mode

Use Mission Control v0 as the operating loop for this audit.
Mission Control is not a replacement for source reads, tests, compiler output, or runtime proof.

Required operating loop:

1. Establish trust with `trust_selftest`, or `session_handshake` scoped to this repo.
2. If mission tools are not visible in this host, record `mission_control_unavailable=true`, fall back to the `m1nd-trained` loop, and do not fake mission calls.
3. Start a repo-scoped mission with `mission_start`: `agent_id=<lane_id>`, `repo=<workspace>`, `task="bug-hunt audit for behavioral defects"`, `mode="bug_hunt"`, `budget="normal"`, and `risk="medium"`.
4. Take the starter move, then call `mission_next` after each meaningful action with a concise `last_event` summary.
5. Treat `do_not` entries from `mission_next` as guardrails. If you disagree, record a dissent event explaining the chosen tool and required evidence.
6. When `mission_next` switches to direct proof, stop graph exploration and use direct source reads, rg, tests, compiler output, or focused runtime probes.
7. Call `mission_verify` before finalizing material findings. If a claim is rejected or needs evidence, gather that evidence or lower the confidence.
8. Call `mission_close` before writing the final lane JSON; preserve gaps, non-claims, and proof-packet summary.
9. If using local `probe_m1nd.py` in this benchmark workspace, pass `--no-worktree-artifacts --workspace-root <repo>` unless intentionally debugging runtime sidecar state.
10. Fill `mission_control_usage` in the lane result with `mission_id`, route, call counts, unavailable state, `do_not` guardrails, verified/rejected claims, direct-proof switches, and proof-packet summary.
11. Also preserve raw m1nd calls in `m1nd_usage` when useful for auditability.

## Required Output

Write your final JSON result to `/Users/kle1nz/m1nd/docs/benchmarks/bug-hunt-rounds/bughunt-p-limit-mc0-20260517T134906Z/lane-results/audit-04.json`.
Append investigation events to `/Users/kle1nz/m1nd/docs/benchmarks/bug-hunt-rounds/bughunt-p-limit-mc0-20260517T134906Z/event-streams/audit-04.jsonl` using `event_source="agent"`.
Every event must include `schema`, `round_id`, `lane_id`, `event_source`, `event_type`, and `created_at`.
Record at least `audit_started`, one first-discovery event such as `findings_identified`, `focused_probes`, or `runtime_probe`, and `result_written`.
Use ISO timestamps; do not use `ts` or `event` as substitutes in new rounds.
Use the schema in `lane-result-template.json`.

Findings should include title, severity, file, symbol, cause, impact, evidence, reproduction_or_test, and confidence.
Extra findings are welcome, but they must be concrete and source-backed.
