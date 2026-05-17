# Bug-Hunt Audit Lane: audit-02

Round: `bughunt-p-limit-mc0-rerun-20260517T150238Z`
Repo: `p-limit`
Instruction mode: `m1nd-mission-control`
Workspace: `/Users/kle1nz/m1nd/.m1nd-field-workspaces/bughunt-p-limit-mc0-rerun-20260517T150238Z/audit-02/p-limit`

Work as if this is a real production-minded dependency audit.
Do not guess the benchmark hypothesis, bug count, or comparison arm.
Find real behavioral defects, edge-case regressions, missing tests, or contract mismatches.
Do not patch files. Do not read `operator-only/` artifacts.

## m1nd Mission Control Mode

Use Mission Control v0 as the operating loop for this audit.
Mission Control is not a replacement for source reads, tests, compiler output, or runtime proof.

Required operating loop:

1. Establish trust with `trust_selftest`, or `session_handshake` scoped to this repo.
2. If native mission tools are not visible in this host, probe the selected runtime with local `probe_m1nd.py tools`. If that helper surface includes `mission_start`, `mission_next`, `mission_verify`, and `mission_close`, use `probe_m1nd.py --runtime-dir /Users/kle1nz/m1nd/docs/benchmarks/bug-hunt-rounds/bughunt-p-limit-mc0-rerun-20260517T150238Z/m1nd-runtime/audit-02 --workspace-root <repo> call <tool> <json>` for every Mission Control call, so mission state survives across calls. Record `mission_transport="probe_helper_stdio"`.
3. Record `mission_control_unavailable=true` only when neither the native host surface nor the helper surface can call Mission Control. Then fall back to the `m1nd-trained` loop and do not fake mission calls.
4. Start a repo-scoped mission with `mission_start`: `agent_id=<lane_id>`, `repo=<workspace>`, `task="bug-hunt audit for behavioral defects"`, `mode="bug_hunt"`, `budget="normal"`, and `risk="medium"`.
5. Take the starter move, then call `mission_next` after each meaningful action with a concise `last_event` summary.
6. Treat `do_not` entries from `mission_next` as guardrails. If you disagree, record a dissent event explaining the chosen tool and required evidence.
7. When `mission_next` switches to direct proof, stop graph exploration and use direct source reads, rg, tests, compiler output, or focused runtime probes.
8. Call `mission_verify` before finalizing material findings. If a claim is rejected or needs evidence, gather that evidence or lower the confidence.
9. Call `mission_close` before writing the final lane JSON; preserve gaps, non-claims, and proof-packet summary.
10. If using local `probe_m1nd.py` in this benchmark workspace, pass `--no-worktree-artifacts --workspace-root <repo>` unless intentionally debugging runtime sidecar state.
11. Fill `mission_control_usage` in the lane result with `mission_id`, route, transport, call counts, unavailable state, `do_not` guardrails, verified/rejected claims, direct-proof switches, and proof-packet summary.
12. Also preserve raw m1nd calls in `m1nd_usage` when useful for auditability.

## Required Output

Write your final JSON result to `/Users/kle1nz/m1nd/docs/benchmarks/bug-hunt-rounds/bughunt-p-limit-mc0-rerun-20260517T150238Z/lane-results/audit-02.json`.
Append investigation events to `/Users/kle1nz/m1nd/docs/benchmarks/bug-hunt-rounds/bughunt-p-limit-mc0-rerun-20260517T150238Z/event-streams/audit-02.jsonl` using `event_source="agent"`.
Every event must include `schema`, `round_id`, `lane_id`, `event_source`, `event_type`, and `created_at`.
Record at least `audit_started`, one first-discovery event such as `findings_identified`, `focused_probes`, or `runtime_probe`, and `result_written`.
Use ISO timestamps; do not use `ts` or `event` as substitutes in new rounds.
Use the schema in `lane-result-template.json`.

Findings should include title, severity, file, symbol, cause, impact, evidence, reproduction_or_test, and confidence.
Extra findings are welcome, but they must be concrete and source-backed.
