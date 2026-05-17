# Bug-Hunt Audit Lane: audit-01

Round: `bughunt-p-limit-mc0-20260517T134906Z`
Repo: `p-limit`
Instruction mode: `m1nd-short-audit`
Workspace: `/Users/kle1nz/m1nd/.m1nd-field-workspaces/bughunt-p-limit-mc0-20260517T134906Z/audit-01/p-limit`

Work as if this is a real production-minded dependency audit.
Do not guess the benchmark hypothesis, bug count, or comparison arm.
Find real behavioral defects, edge-case regressions, missing tests, or contract mismatches.
Do not patch files. Do not read `operator-only/` artifacts.

## m1nd Short-Audit Mode

Use m1nd as a bounded orientation pass, then move quickly to direct source and runtime proof.
This mode is for small or localized audit tasks where full graph navigation may cost more than it returns.

Short-audit loop:

1. Establish trust with `trust_selftest`, or `session_handshake` scoped to this repo.
2. Prefer `probe_m1nd.py --no-worktree-artifacts --workspace-root <repo> short-audit --agent-id <lane> --repo <repo> --query <focused query> --tool search` so trust, ingest when needed, one cheap orientation call, and the direct-proof handoff happen in one MCP process.
3. Spend a fixed small budget on m1nd: at most one helper call plus, only if it produced concrete leads, one additional focused orientation call.
4. Record `m1nd_usage_mode="short_audit_orientation"` in notes or `m1nd_usage`.
5. After that budget, switch to direct source reads, git diff, focused runtime probes, tests, or compiler output.
6. If m1nd is blocked, stale, or noisy after the bounded pass, record `recovery_overhead` and continue directly.
7. Do not keep exploring the graph after concrete suspect files and behaviors are visible.
8. Record m1nd calls, recovery path, files inspected, commands run, direct probes, fallback reason, and where short-audit helped or hurt.

## Required Output

Write your final JSON result to `/Users/kle1nz/m1nd/docs/benchmarks/bug-hunt-rounds/bughunt-p-limit-mc0-20260517T134906Z/lane-results/audit-01.json`.
Append investigation events to `/Users/kle1nz/m1nd/docs/benchmarks/bug-hunt-rounds/bughunt-p-limit-mc0-20260517T134906Z/event-streams/audit-01.jsonl` using `event_source="agent"`.
Every event must include `schema`, `round_id`, `lane_id`, `event_source`, `event_type`, and `created_at`.
Record at least `audit_started`, one first-discovery event such as `findings_identified`, `focused_probes`, or `runtime_probe`, and `result_written`.
Use ISO timestamps; do not use `ts` or `event` as substitutes in new rounds.
Use the schema in `lane-result-template.json`.

Findings should include title, severity, file, symbol, cause, impact, evidence, reproduction_or_test, and confidence.
Extra findings are welcome, but they must be concrete and source-backed.
