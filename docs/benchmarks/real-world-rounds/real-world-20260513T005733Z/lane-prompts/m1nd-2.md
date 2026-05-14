# Real-World Agent Lane: m1nd-2

Round: `real-world-20260513T005733Z`
Arm: `m1nd_available`

Use m1nd first for orientation, localization, impact, connected context, docs/code binding, and risky change prep. If m1nd is blocked or stale, record the recovery path and verify final truth with files, tests, and git.

Do not guess the benchmark hypothesis. Work as if this is a normal coding task.
Keep public claims out of the result. Record missing proof instead of smoothing it away.
Do not commit, publish, or push fixture repo changes.

## Fixture Repositories

- click-python-cli (python): `.m1nd-benchmark-fixtures/real-world/click-python-cli`
- p-limit-node (typescript): `.m1nd-benchmark-fixtures/real-world/p-limit-node`
- human-panic-rust-cli (rust): `.m1nd-benchmark-fixtures/real-world/human-panic-rust-cli`

## Isolated Lane Workspaces

Use your isolated workspace paths for patch tasks. Do not edit shared fixture repos.
- click-python-cli: `.m1nd-benchmark-fixtures/real-world-lanes/real-world-20260513T005733Z/m1nd-2/click-python-cli`
- p-limit-node: `.m1nd-benchmark-fixtures/real-world-lanes/real-world-20260513T005733Z/m1nd-2/p-limit-node`
- human-panic-rust-cli: `.m1nd-benchmark-fixtures/real-world-lanes/real-world-20260513T005733Z/m1nd-2/human-panic-rust-cli`

If a fixture is missing, clone it from the URL in `round.json` or mark the affected task invalidated.

## Task Battery

- repo_architecture_audit on `click-python-cli`: Explain the repo architecture, main modules, entrypoints, data/control flow, and top risks. Expected evidence: main entrypoints named, module boundaries named, at least two real file references, risk list separates proven facts from hypotheses.
- feature_location on `p-limit-node`: Find where a named feature or public behavior is implemented and identify the tests that protect it. Expected evidence: implementation file named, test file named or missing test stated, false-positive files avoided.
- flow_explanation on `human-panic-rust-cli`: Explain a realistic request/command/API flow from public entrypoint to internal behavior. Expected evidence: entrypoint named, intermediate calls named, observable output or side effect named.
- bug_symptom_triage on `click-python-cli`: Given a realistic symptom, isolate the most likely fault boundary and name the next verification step. Expected evidence: most likely fault file or function named, alternative theory preserved or rejected, next command/test/file named.
- safe_change_plan on `p-limit-node`: Plan a small behavior change, including blast radius, files to edit, and proof gates. Expected evidence: edit targets named, downstream callers or tests named, risky assumptions explicit.
- small_feature_patch on `human-panic-rust-cli`: Implement a tiny feature or option consistent with local style and run focused checks. Expected evidence: minimal patch, test or example updated when appropriate, focused check result recorded.
- seeded_bug_fix on `click-python-cli`: Fix a seeded or clearly described bug without broad refactors. Expected evidence: root cause named, patch is scoped, regression proof recorded.
- bounded_refactor_plan on `p-limit-node`: Prepare a bounded refactor and identify hidden coupling before any edit. Expected evidence: coupled files named, safe ordering proposed, rollback or proof boundary named.
- code_review_diff on `human-panic-rust-cli`: Review a supplied or seeded diff for real bugs, regressions, and missing tests. Expected evidence: findings ordered by severity, file/line references when available, style-only comments avoided.
- docs_drift_check on `click-python-cli`: Compare README/docs claims against implementation and identify drift or missing documentation. Expected evidence: claim source named, code truth named, drift or no-drift conclusion justified.

## Required Result

Fill a JSON result using `lane-result-template.json`.
