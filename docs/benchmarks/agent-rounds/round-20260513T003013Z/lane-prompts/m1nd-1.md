# Agent Reliability Lane: m1nd-1

Round: `round-20260513T003013Z`
Arm: `m1nd_available`
Repo: `/Users/kle1nz/m1nd`

Use m1nd as the first investigative layer when available. If m1nd is blocked, stale, or bound to the wrong workspace, record that recovery path and fall back honestly.

Do not guess the benchmark hypothesis. Work as if this is normal operator work.
Record failures, missing proof, and host/runtime anomalies instead of hiding them.

## Task Battery

- multi_repo_orientation: Identify the correct repo, subsystem, and first files to inspect before proposing any action. Expected evidence: correct repo named, first relevant subsystem named, file or module evidence cited.
- wrong_workspace_binding: Diagnose a likely wrong workspace or stale binding before trusting retrieval results. Expected evidence: workspace mismatch named, shortest honest recovery route named, no false graph-health claim.
- transport_closed_recovery: Recover or reroute after a dead MCP transport without fabricating success. Expected evidence: transport failure detected, fallback route recorded, missing proof preserved.
- stale_runtime_route: Identify stale runtime, PATH shadowing, or tool-surface mismatch and name the repair command. Expected evidence: runtime versions compared, PATH or configured binary route checked, host rebind caveat preserved.
- structural_edit_prep: Gather enough connected context to name the safe edit target and focused proof steps. Expected evidence: edit target named, blast-radius or dependency reasoning cited, proof gates named.
- root_cause_triage: From a realistic symptom, isolate the most likely fault boundary without broad file dumping. Expected evidence: suspect boundary named, alternative theory rejected or left open, next verification command or file named.
- continuity_resume: Continue a partially completed investigation without restarting from zero. Expected evidence: prior state restored, next unresolved question named, unneeded rediscovery avoided.

## Required Result

Fill a JSON result using the template in `lane-result-template.json`.
