#!/usr/bin/env python3
"""Create and score real-world agent work benchmark rounds.

This harness tests normal coding-agent work on external repositories. It is not
host-recovery specific; it compares m1nd-assisted and control lanes on everyday
tasks such as architecture audit, feature localization, bug triage, change
planning, review, and docs drift.
"""

import argparse
import json
import shutil
import statistics
import subprocess
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path


ROUND_SCHEMA = "m1nd-real-world-agent-round-v0"
LANE_SCHEMA = "m1nd-real-world-agent-lane-result-v0"
REPORT_SCHEMA = "m1nd-real-world-agent-report-v0"
EVENT_SCHEMA = "m1nd-real-world-agent-event-v0"
ANSWER_KEY_SCHEMA = "m1nd-real-world-agent-answer-key-v0"
FIXTURE_LOCK_SCHEMA = "m1nd-real-world-agent-fixture-lock-v0"
JUDGE_INPUT_SCHEMA = "m1nd-real-world-agent-judge-input-v0"

PRIMARY_ARMS = ("m1nd_available", "no_m1nd")
VALID_ARMS = (*PRIMARY_ARMS, "adjudication")
VALID_FINAL_STATES = ("success", "partial", "failed", "invalidated")
VALID_TASK_MODES = ("audit", "localize", "explain", "diagnose", "plan", "patch", "review", "docs")
VALID_EVENT_TYPES = (
    "lane_assigned",
    "m1nd_call",
    "shell_command",
    "file_read",
    "test_run",
    "patch_write",
    "recovery_step",
    "finding",
    "handoff",
)
VALID_COMPARABILITY_CLASSES = ("comparable", "partial", "excluded", "needs_review")
SCORE_KEYS = (
    "orientation",
    "localization",
    "causal_understanding",
    "proof",
    "efficiency",
    "outcome",
)

NON_CLAIMS = [
    "one real-world round is not a public performance claim",
    "benchmark repos are fixtures, not proof of universal repo performance",
    "m1nd does not replace tests, compiler output, git history, rg, or direct file truth",
    "agent testimony is not evidence without scored task artifacts",
    "warm-graph and cold-graph results must be reported separately",
    "a correct plan is not the same evidence as a correct patch",
]

DEFAULT_REPOS = [
    {
        "repo_id": "click-python-cli",
        "url": "https://github.com/pallets/click.git",
        "ecosystem": "python",
        "why": "mature Python CLI library with docs, tests, commands, options, and parser internals",
    },
    {
        "repo_id": "p-limit-node",
        "url": "https://github.com/sindresorhus/p-limit.git",
        "ecosystem": "typescript",
        "why": "small TypeScript utility with compact source, tests, package metadata, and public API docs",
    },
    {
        "repo_id": "human-panic-rust-cli",
        "url": "https://github.com/rust-cli/human-panic.git",
        "ecosystem": "rust",
        "why": "Rust library/CLI-adjacent crate with examples, feature flags, tests, and public docs",
    },
]

DEFAULT_TASKS = [
    {
        "task_id": "repo_architecture_audit",
        "mode": "audit",
        "title": "Architecture audit",
        "prompt": "Explain the repo architecture, main modules, entrypoints, data/control flow, and top risks.",
        "expected_evidence": [
            "main entrypoints named",
            "module boundaries named",
            "at least two real file references",
            "risk list separates proven facts from hypotheses",
        ],
        "requires_code_change": False,
    },
    {
        "task_id": "feature_location",
        "mode": "localize",
        "title": "Feature localization",
        "prompt": "Find where a named feature or public behavior is implemented and identify the tests that protect it.",
        "expected_evidence": [
            "implementation file named",
            "test file named or missing test stated",
            "false-positive files avoided",
        ],
        "requires_code_change": False,
    },
    {
        "task_id": "flow_explanation",
        "mode": "explain",
        "title": "End-to-end flow explanation",
        "prompt": "Explain a realistic request/command/API flow from public entrypoint to internal behavior.",
        "expected_evidence": [
            "entrypoint named",
            "intermediate calls named",
            "observable output or side effect named",
        ],
        "requires_code_change": False,
    },
    {
        "task_id": "bug_symptom_triage",
        "mode": "diagnose",
        "title": "Bug symptom triage",
        "prompt": "Given a realistic symptom, isolate the most likely fault boundary and name the next verification step.",
        "expected_evidence": [
            "most likely fault file or function named",
            "alternative theory preserved or rejected",
            "next command/test/file named",
        ],
        "requires_code_change": False,
    },
    {
        "task_id": "safe_change_plan",
        "mode": "plan",
        "title": "Safe change plan",
        "prompt": "Plan a small behavior change, including blast radius, files to edit, and proof gates.",
        "expected_evidence": [
            "edit targets named",
            "downstream callers or tests named",
            "risky assumptions explicit",
        ],
        "requires_code_change": False,
    },
    {
        "task_id": "small_feature_patch",
        "mode": "patch",
        "title": "Small feature patch",
        "prompt": "Implement a tiny feature or option consistent with local style and run focused checks.",
        "expected_evidence": [
            "minimal patch",
            "test or example updated when appropriate",
            "focused check result recorded",
        ],
        "requires_code_change": True,
    },
    {
        "task_id": "seeded_bug_fix",
        "mode": "patch",
        "title": "Seeded bug fix",
        "prompt": "Fix a seeded or clearly described bug without broad refactors.",
        "expected_evidence": [
            "root cause named",
            "patch is scoped",
            "regression proof recorded",
        ],
        "requires_code_change": True,
    },
    {
        "task_id": "bounded_refactor_plan",
        "mode": "plan",
        "title": "Bounded refactor plan",
        "prompt": "Prepare a bounded refactor and identify hidden coupling before any edit.",
        "expected_evidence": [
            "coupled files named",
            "safe ordering proposed",
            "rollback or proof boundary named",
        ],
        "requires_code_change": False,
    },
    {
        "task_id": "code_review_diff",
        "mode": "review",
        "title": "Code review of a diff",
        "prompt": "Review a supplied or seeded diff for real bugs, regressions, and missing tests.",
        "expected_evidence": [
            "findings ordered by severity",
            "file/line references when available",
            "style-only comments avoided",
        ],
        "requires_code_change": False,
    },
    {
        "task_id": "docs_drift_check",
        "mode": "docs",
        "title": "Docs/spec drift check",
        "prompt": "Compare README/docs claims against implementation and identify drift or missing documentation.",
        "expected_evidence": [
            "claim source named",
            "code truth named",
            "drift or no-drift conclusion justified",
        ],
        "requires_code_change": False,
    },
]

TASK_PAYLOADS = {
    "repo_architecture_audit": {
        "repo_id": "click-python-cli",
        "payload_id": "click-architecture-v1",
        "focus": "Audit Click's public export layer, decorators, command core, parser, and testing harness.",
        "must_cover": [
            "public API re-exports",
            "command and group invocation path",
            "parameter/type conversion",
            "test runner IO isolation",
        ],
    },
    "feature_location": {
        "repo_id": "p-limit-node",
        "payload_id": "p-limit-clear-queue-reject-on-clear-v1",
        "feature": "The rejectOnClear and clearQueue behavior for pending tasks.",
        "must_find": [
            "runtime implementation",
            "type definition",
            "test coverage",
            "README/API docs",
        ],
    },
    "flow_explanation": {
        "repo_id": "human-panic-rust-cli",
        "payload_id": "human-panic-release-panic-flow-v1",
        "flow": "Explain what happens when setup_panic!() is installed and a release-mode panic occurs.",
        "must_cover": [
            "public macro or setup entrypoint",
            "panic hook behavior",
            "report writing path",
            "observable user-facing output",
        ],
    },
    "bug_symptom_triage": {
        "repo_id": "click-python-cli",
        "payload_id": "click-callable-instance-type-triage-v1",
        "symptom": (
            "A callable instance used as a custom Click option type crashes during command "
            "construction with AttributeError because the object has no __name__ attribute."
        ),
        "must_answer": [
            "most likely fault boundary",
            "why it is not a parser/runtime invocation issue",
            "next focused regression test",
        ],
    },
    "safe_change_plan": {
        "repo_id": "p-limit-node",
        "payload_id": "p-limit-clear-queue-return-count-plan-v1",
        "change_request": (
            "Plan a backwards-compatible change so clearQueue() returns the number of pending "
            "tasks it discarded or rejected, without touching already running tasks."
        ),
        "must_cover": [
            "runtime edit target",
            "types/docs/test targets",
            "rejectOnClear behavior",
            "no change to activeCount semantics",
        ],
    },
    "small_feature_patch": {
        "repo_id": "human-panic-rust-cli",
        "payload_id": "human-panic-metadata-name-version-builders-v1",
        "change_request": (
            "Add Metadata::name(...) and Metadata::version(...) builder methods that preserve "
            "the existing non-empty string guard style."
        ),
        "must_cover": [
            "minimal implementation",
            "focused unit tests",
            "no public panic/report behavior rewrite",
        ],
    },
    "seeded_bug_fix": {
        "repo_id": "click-python-cli",
        "payload_id": "click-seeded-callable-instance-type-fix-v1",
        "seeded_artifact_id": "click-callable-instance-type-test-v1",
        "bug": (
            "The lane workspace contains a seeded regression test proving callable instances "
            "should work as custom option types."
        ),
        "must_cover": [
            "root cause",
            "minimal fix",
            "seeded regression test result",
        ],
    },
    "bounded_refactor_plan": {
        "repo_id": "p-limit-node",
        "payload_id": "p-limit-queue-scheduling-refactor-plan-v1",
        "refactor_scope": "Queue scheduling and draining helpers only.",
        "must_cover": [
            "resumeNext",
            "next",
            "enqueue",
            "clearQueue",
            "concurrency setter",
        ],
    },
    "code_review_diff": {
        "repo_id": "human-panic-rust-cli",
        "payload_id": "human-panic-review-diff-v1",
        "supplied_diff": "benchmark-payloads/review-diff-human-panic.patch",
        "review_focus": "Find real user-visible regressions and missing tests in the supplied diff.",
        "must_cover": [
            "duplicate or noisy support output when homepage and repository coexist",
            "missing regression test",
            "avoid style-only findings",
        ],
    },
    "docs_drift_check": {
        "repo_id": "click-python-cli",
        "payload_id": "click-lazy-loading-docs-drift-v1",
        "claim": "README/docs say Click supports lazy loading of subcommands at runtime.",
        "must_compare": [
            "README and docs/index claim",
            "docs/complex lazy loading pattern",
            "actual Group behavior",
        ],
    },
}

TASK_ANSWER_KEYS = {
    "repo_architecture_audit": {
        "expected_files": [
            "src/click/__init__.py",
            "src/click/decorators.py",
            "src/click/core.py",
            "src/click/parser.py",
            "src/click/testing.py",
        ],
        "expected_points": [
            "Click's public surface is re-exported from src/click/__init__.py.",
            "Decorators assemble commands and parameters before core invocation.",
            "testing.py owns CliRunner and IO isolation risks.",
        ],
    },
    "feature_location": {
        "expected_files": ["index.js", "index.d.ts", "test.js", "readme.md"],
        "expected_points": [
            "clearQueue lives on the returned generator function.",
            "rejectOnClear rejects queued pending promises while preserving running tasks.",
            "Types/docs/tests all need mention for behavior changes.",
        ],
    },
    "flow_explanation": {
        "expected_files": ["src/lib.rs", "src/panic.rs", "src/report.rs"],
        "expected_points": [
            "setup_panic! installs a panic hook.",
            "panic.rs formats the user-facing message and report path.",
            "report.rs is the report serialization/writing boundary.",
        ],
    },
    "bug_symptom_triage": {
        "expected_files": ["src/click/types.py"],
        "expected_symbols": ["FuncParamType.__init__"],
        "expected_points": [
            "The constructor assumes func.__name__ exists.",
            "The fault happens before parsing, during type construction/registration.",
        ],
    },
    "safe_change_plan": {
        "expected_files": ["index.js", "index.d.ts", "test.js", "readme.md"],
        "expected_points": [
            "Return queued size from clearQueue before clearing.",
            "Keep active/running promises untouched.",
            "Cover rejectOnClear true and false.",
        ],
    },
    "small_feature_patch": {
        "expected_files": ["src/metadata.rs"],
        "expected_symbols": ["Metadata::name", "Metadata::version"],
        "expected_points": [
            "Mirror existing builder style.",
            "Preserve non-empty guard.",
            "Add focused tests near metadata builder tests.",
        ],
    },
    "seeded_bug_fix": {
        "expected_files": [
            "src/click/types.py",
            "tests/test_m1nd_seeded_callable_type.py",
        ],
        "expected_symbols": ["FuncParamType.__init__"],
        "expected_points": [
            "Use a fallback name for callable instances without __name__.",
            "Seeded test must pass after the patch.",
        ],
    },
    "bounded_refactor_plan": {
        "expected_files": ["index.js", "test.js"],
        "expected_symbols": ["resumeNext", "next", "enqueue", "clearQueue", "concurrency"],
        "expected_points": [
            "Refactor must not alter queue scheduling semantics.",
            "Concurrency setter and clearQueue are coupled to scheduling state.",
        ],
    },
    "code_review_diff": {
        "expected_files": ["src/panic.rs"],
        "expected_points": [
            "The diff makes support output noisier by printing repository even when homepage exists.",
            "If homepage and repository duplicate the same URL, output can become redundant.",
            "Needs a focused regression test around support lines.",
        ],
    },
    "docs_drift_check": {
        "expected_files": ["README.md", "docs/index.rst", "docs/complex.rst", "src/click/core.py"],
        "expected_points": [
            "The high-level claim is true only with the documented LazyGroup/subclass pattern.",
            "Built-in Group is not itself a magic lazy loader for arbitrary subcommands.",
            "The correct conclusion should preserve nuance rather than call the docs simply false.",
        ],
    },
}

REVIEW_DIFF_HUMAN_PANIC = """diff --git a/src/panic.rs b/src/panic.rs
--- a/src/panic.rs
+++ b/src/panic.rs
@@
-    if let Some(homepage) = homepage {
-        writeln!(buffer, "- Homepage: {homepage}")?;
-    } else if let Some(repository) = repository {
+    if let Some(homepage) = homepage {
+        writeln!(buffer, "- Homepage: {homepage}")?;
+    }
+    if let Some(repository) = repository {
         writeln!(buffer, "- Repository: {repository}")?;
     }
"""

CLICK_CALLABLE_TYPE_TEST = '''import click
from click.testing import CliRunner


class UpperCase:
    def __call__(self, value: str) -> str:
        return value.upper()


def test_callable_instance_type_has_stable_name():
    @click.command()
    @click.option("--value", type=UpperCase())
    def cli(value):
        click.echo(value)

    result = CliRunner().invoke(cli, ["--value", "hello"])

    assert result.exit_code == 0
    assert result.output == "HELLO\\n"
'''


def now_iso():
    return datetime.now(timezone.utc).isoformat()


def dump_json(path: Path, payload):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, ensure_ascii=False)
        handle.write("\n")


def append_jsonl(path: Path, payload):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(payload, ensure_ascii=False, sort_keys=True))
        handle.write("\n")


def load_json(path: Path):
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def resolve_path(path, base_dir: Path):
    candidate = Path(path)
    if not candidate.is_absolute():
        candidate = base_dir / candidate
    return candidate


def median(values):
    values = [value for value in values if isinstance(value, (int, float))]
    if not values:
        return None
    return round(statistics.median(values), 3)


def safe_rate(numerator, denominator):
    if denominator:
        return round(numerator / denominator, 4)
    return None


def lane_plan():
    lanes = []
    for index in range(1, 4):
        lanes.append({"lane_id": f"m1nd-{index}", "arm": "m1nd_available"})
    for index in range(1, 4):
        lanes.append({"lane_id": f"control-{index}", "arm": "no_m1nd"})
    lanes.append({"lane_id": "judge-1", "arm": "adjudication"})
    return lanes


def repo_local_path(fixtures_dir: Path, repo):
    return fixtures_dir / repo["repo_id"]


def git_head_commit(repo_path: Path):
    if not repo_path.exists():
        return None
    result = run_command(["git", "rev-parse", "HEAD"], cwd=repo_path)
    if result["returncode"] != 0 or not result["stdout"]:
        return None
    return result["stdout"].splitlines()[0].strip()


def repo_revision_metadata(repo_path: Path):
    head_commit = git_head_commit(repo_path)
    return {
        "fixture_present": repo_path.exists(),
        "head_commit": head_commit,
        "head_commit_short": head_commit[:12] if head_commit else None,
    }


def repo_manifest(fixtures_dir: Path):
    repos = []
    for repo in DEFAULT_REPOS:
        item = dict(repo)
        local_path = repo_local_path(fixtures_dir, repo)
        item["local_path"] = str(local_path)
        item["fixture_present"] = local_path.exists()
        item["revision"] = repo_revision_metadata(local_path)
        repos.append(item)
    return repos


def public_task_payload(task):
    payload = TASK_PAYLOADS.get(task["task_id"], {})
    public = dict(task)
    public["repo_id"] = payload.get("repo_id")
    public["payload_id"] = payload.get("payload_id")
    public["payload"] = {
        key: value
        for key, value in payload.items()
        if key not in {"repo_id", "payload_id"}
    }
    return public


def fixture_lock_entry(repo):
    revision = repo.get("revision") or {}
    return {
        "repo_id": repo["repo_id"],
        "ecosystem": repo["ecosystem"],
        "url": repo["url"],
        "local_path": repo["local_path"],
        "fixture_present": bool(repo.get("fixture_present")),
        "lock_commit": revision.get("head_commit"),
        "revision": revision,
    }


def fixture_lock_payload(round_id, repos):
    return {
        "schema": FIXTURE_LOCK_SCHEMA,
        "round_id": round_id,
        "generated_at": now_iso(),
        "fixture_repos": [fixture_lock_entry(repo) for repo in repos],
        "non_claims": [
            "fixture lock captures the current local fixture refs at init time",
            "fixture lock does not prove external repos are immutable unless the locked commits exist and are fetched",
            "fixture lock is operator-only benchmark metadata, not agent prompt content",
        ],
    }


def answer_key_payload(round_id, repos):
    return {
        "schema": ANSWER_KEY_SCHEMA,
        "round_id": round_id,
        "generated_at": now_iso(),
        "fixture_repos": [
            {
                "repo_id": repo["repo_id"],
                "ecosystem": repo["ecosystem"],
                "local_path": repo["local_path"],
                "fixture_present": bool(repo.get("fixture_present")),
                "lock_commit": (repo.get("revision") or {}).get("head_commit"),
                "revision": repo.get("revision"),
            }
            for repo in repos
        ],
        "tasks": [
            {
                "task_id": task["task_id"],
                "repo_id": TASK_PAYLOADS.get(task["task_id"], {}).get("repo_id"),
                "payload_id": TASK_PAYLOADS.get(task["task_id"], {}).get("payload_id"),
                "answer_key": TASK_ANSWER_KEYS.get(task["task_id"], {}),
            }
            for task in DEFAULT_TASKS
        ],
        "non_claims": [
            "operator-only/answer-key.json is for parent and adjudicator use, not for primary benchmark lanes",
            "the answer key does not replace direct proof from fixture files and tests",
        ],
    }


def event_log_path_for_lane(lane):
    return f"event-streams/{lane['lane_id']}.jsonl"


def initial_event(round_payload, lane):
    return {
        "schema": EVENT_SCHEMA,
        "round_id": round_payload["round_id"],
        "lane_id": lane["lane_id"],
        "event_type": "lane_assigned",
        "event_source": "harness",
        "timestamp": now_iso(),
        "task_id": None,
        "repo_id": None,
        "summary": "Lane prompt, result template, and event stream were created.",
        "files": [
            f"lane-prompts/{lane['lane_id']}.md",
            f"lane-results/{lane['lane_id']}.json",
        ],
        "command": None,
        "exit_code": None,
        "m1nd_tool": None,
        "proof_ref": None,
        "notes": "Agents should append JSONL events with event_source=agent.",
    }


def write_initial_event_stream(out_dir: Path, round_payload, lane):
    event_path = out_dir / event_log_path_for_lane(lane)
    if event_path.exists():
        event_path.unlink()
    append_jsonl(event_path, initial_event(round_payload, lane))
    return event_path


def write_benchmark_payloads(out_dir: Path):
    payload_dir = out_dir / "benchmark-payloads"
    payload_dir.mkdir(parents=True, exist_ok=True)
    review_diff_path = payload_dir / "review-diff-human-panic.patch"
    review_diff_path.write_text(REVIEW_DIFF_HUMAN_PANIC, encoding="utf-8")
    return [
        {
            "artifact_id": "human-panic-review-diff-v1",
            "path": str(review_diff_path),
            "purpose": "supplied diff for code_review_diff task",
        }
    ]


def task_repo_pairs(repos):
    repo_by_id = {repo["repo_id"]: repo for repo in repos}
    pairs = []
    for index, task in enumerate(DEFAULT_TASKS):
        public_task = public_task_payload(task)
        repo = repo_by_id.get(public_task.get("repo_id")) or repos[index % len(repos)]
        pairs.append({"task": public_task, "repo": repo})
    return pairs


def lane_prompt(round_payload, lane):
    if lane["arm"] == "m1nd_available":
        tool_rule = (
            "Use m1nd first for orientation, localization, impact, connected context, "
            "docs/code binding, and risky change prep. If m1nd is blocked or stale, "
            "record the recovery path and verify final truth with files, tests, and git."
        )
    elif lane["arm"] == "no_m1nd":
        tool_rule = (
            "Do not use m1nd. Use normal coding-agent investigation: file reads, rg, "
            "git, tests, compiler output, and direct repo inspection."
        )
    else:
        tool_rule = (
            "Adjudicate after primary lanes finish. Do not change primary artifacts; "
            "judge correctness, overclaiming, evidence quality, and comparability."
        )

    repo_lines = []
    for repo in round_payload["fixture_repos"]:
        repo_lines.append(
            f"- {repo['repo_id']} ({repo['ecosystem']}): `{repo['local_path']}`"
        )
    lane_workspace_root = round_payload.get("lane_workspace_root")
    lane_workspace_lines = []
    if lane_workspace_root:
        for repo in round_payload["fixture_repos"]:
            lane_workspace_lines.append(
                f"- {repo['repo_id']}: `{lane_workspace_root}/{lane['lane_id']}/{repo['repo_id']}`"
            )

    task_lines = []
    for pair in round_payload["task_matrix"]:
        task = pair["task"]
        repo = pair["repo"]
        evidence = ", ".join(task["expected_evidence"])
        payload_json = json.dumps(task.get("payload", {}), ensure_ascii=False, sort_keys=True)
        task_lines.append(
            f"- {task['task_id']} on `{repo['repo_id']}`: {task['prompt']} "
            f"Fixed payload: `{payload_json}` Expected evidence: {evidence}."
        )
    adjudication_line = ""
    if lane["arm"] == "adjudication":
        adjudication_line = (
            "Judge-only: fill top-level adjudications[] with primary_lane_id, task_id, "
            "adjudicated_final_state, adjudicated_scores, comparability_class, "
            "exclusion_reason, notes, and evidence."
        )

    return "\n".join(
        [
            f"# Real-World Agent Lane: {lane['lane_id']}",
            "",
            f"Round: `{round_payload['round_id']}`",
            f"Arm: `{lane['arm']}`",
            "",
            tool_rule,
            "",
            "Do not guess the benchmark hypothesis. Work as if this is a normal coding task.",
            "Keep public claims out of the result. Record missing proof instead of smoothing it away.",
            "Do not commit, publish, or push fixture repo changes.",
            "",
            "## Fixture Repositories",
            "",
            *repo_lines,
            "",
            "## Isolated Lane Workspaces",
            "",
            "Use your isolated workspace paths for patch tasks. Do not edit shared fixture repos.",
            *lane_workspace_lines,
            "",
            "If a fixture is missing, clone it from the URL in `round.json` or mark the affected task invalidated.",
            "",
            "## Task Battery",
            "",
            *task_lines,
            "",
            "## Required Result",
            "",
            "Fill a JSON result using `lane-result-template.json`.",
            f"Append raw investigation events to `{event_log_path_for_lane(lane)}`.",
            "Use event_source=agent for events you create. Keep one JSON object per line.",
            adjudication_line,
            "Scores must be integers from 0 to 4. Use `4` for excellent; do not use `5`.",
        ]
    )


def result_template(round_payload, lane):
    task_results = []
    for pair in round_payload["task_matrix"]:
        task = pair["task"]
        repo = pair["repo"]
        task_results.append(
            {
                "task_id": task["task_id"],
                "task_payload_id": task.get("payload_id"),
                "task_payload": task.get("payload", {}),
                "repo_id": repo["repo_id"],
                "mode": task["mode"],
                "final_state": "partial",
                "scores": {key: 0 for key in SCORE_KEYS},
                "time_to_good_context_ms": None,
                "time_to_full_proof_ms": None,
                "false_start_count": 0,
                "files_opened": [],
                "search_iterations": 0,
                "tests_or_commands_run": [],
                "code_changed": False,
                "requires_code_change": bool(task["requires_code_change"]),
                "patch_summary": "",
                "correct_files": [],
                "missed_files": [],
                "false_positive_files": [],
                "claim_overreach": "none",
                "primary_failure_class": None,
                "notes": "",
                "evidence": [],
                "event_refs": [],
                "agent_confidence": "unknown",
            }
        )

    return {
        "schema": LANE_SCHEMA,
        "round_id": round_payload["round_id"],
        "lane_id": lane["lane_id"],
        "arm": lane["arm"],
        "model": "",
        "started_at": "",
        "finished_at": "",
        "event_log_path": event_log_path_for_lane(lane),
        "agent_testimony": "",
        "adjudications": [],
        "task_results": task_results,
    }


def init_round(args):
    out_dir = Path(args.out_dir)
    fixtures_dir = Path(args.fixtures_dir)
    round_id = args.round_id or f"real-world-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}"
    lane_workspace_root = Path(args.lane_fixtures_dir) / round_id
    repos = repo_manifest(fixtures_dir)
    benchmark_payloads = write_benchmark_payloads(out_dir)
    fixture_lock_file = Path("operator-only") / "fixture-lock.json"
    round_payload = {
        "schema": ROUND_SCHEMA,
        "round_id": round_id,
        "created_at": now_iso(),
        "status": "planned",
        "fixture_repos": repos,
        "fixture_lock_file": str(fixture_lock_file),
        "lane_workspace_root": str(lane_workspace_root),
        "benchmark_payloads": benchmark_payloads,
        "task_count": len(DEFAULT_TASKS),
        "task_matrix": task_repo_pairs(repos),
        "lanes": lane_plan(),
        "success_criteria": [
            "correct files or modules identified with evidence",
            "causal explanation matches actual code paths",
            "missing tests and missing proof are named honestly",
            "patch tasks stay minimal and are verified with focused checks",
            "control and m1nd lanes use comparable repo snapshots",
        ],
        "non_claims": NON_CLAIMS,
    }

    dump_json(out_dir / "round.json", round_payload)
    dump_json(out_dir / fixture_lock_file, fixture_lock_payload(round_id, repos))
    dump_json(out_dir / "operator-only" / "answer-key.json", answer_key_payload(round_id, repos))
    dump_json(out_dir / "lane-result-template.json", result_template(round_payload, round_payload["lanes"][0]))
    for lane in round_payload["lanes"]:
        prompt_path = out_dir / "lane-prompts" / f"{lane['lane_id']}.md"
        prompt_path.parent.mkdir(parents=True, exist_ok=True)
        prompt_path.write_text(lane_prompt(round_payload, lane) + "\n", encoding="utf-8")
        write_initial_event_stream(out_dir, round_payload, lane)
        dump_json(out_dir / "lane-results" / f"{lane['lane_id']}.json", result_template(round_payload, lane))

    return {
        "schema": ROUND_SCHEMA,
        "round_id": round_id,
        "out_dir": str(out_dir),
        "fixtures_dir": str(fixtures_dir),
        "lane_workspace_root": str(lane_workspace_root),
        "round_file": str(out_dir / "round.json"),
        "answer_key_file": str(out_dir / "operator-only" / "answer-key.json"),
        "fixture_lock_file": str(out_dir / fixture_lock_file),
        "lane_prompts_dir": str(out_dir / "lane-prompts"),
        "lane_results_dir": str(out_dir / "lane-results"),
        "fixture_repos": repos,
        "task_count": len(DEFAULT_TASKS),
        "lane_count": len(round_payload["lanes"]),
        "non_claims": NON_CLAIMS,
    }


def run_command(command, cwd=None):
    completed = subprocess.run(
        command,
        cwd=cwd,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    return {
        "command": command,
        "returncode": completed.returncode,
        "stdout": completed.stdout.strip(),
        "stderr": completed.stderr.strip(),
    }


def clone_or_update_repo(repo, destination: Path, update=False):
    if destination.exists():
        if not update:
            return {
                "repo_id": repo["repo_id"],
                "path": str(destination),
                "status": "already_present",
                "command_result": None,
            }
        result = run_command(["git", "fetch", "--depth", "1", "origin"], cwd=destination)
        return {
            "repo_id": repo["repo_id"],
            "path": str(destination),
            "status": "updated" if result["returncode"] == 0 else "update_failed",
            "command_result": result,
        }

    destination.parent.mkdir(parents=True, exist_ok=True)
    result = run_command(["git", "clone", "--depth", "1", repo["url"], str(destination)])
    return {
        "repo_id": repo["repo_id"],
        "path": str(destination),
        "status": "cloned" if result["returncode"] == 0 else "clone_failed",
        "command_result": result,
    }


def clone_lane_repo(repo, source_path: Path, destination: Path, force=False):
    if destination.exists():
        if force:
            shutil.rmtree(destination)
        else:
            return {
                "repo_id": repo["repo_id"],
                "path": str(destination),
                "status": "already_present",
                "command_result": None,
            }
    destination.parent.mkdir(parents=True, exist_ok=True)
    source = f"file://{source_path.resolve()}" if source_path.exists() else repo["url"]
    result = run_command(["git", "clone", "--depth", "1", source, str(destination)])
    return {
        "repo_id": repo["repo_id"],
        "path": str(destination),
        "status": "prepared" if result["returncode"] == 0 else "prepare_failed",
        "command_result": result,
    }


def seed_lane_repo(repo, destination: Path):
    if repo["repo_id"] != "click-python-cli" or not destination.exists():
        return []

    test_path = destination / "tests" / "test_m1nd_seeded_callable_type.py"
    test_path.parent.mkdir(parents=True, exist_ok=True)
    test_path.write_text(CLICK_CALLABLE_TYPE_TEST, encoding="utf-8")
    return [
        {
            "artifact_id": "click-callable-instance-type-test-v1",
            "task_id": "seeded_bug_fix",
            "path": str(test_path),
            "purpose": "seeded regression test for callable instance custom option types",
        }
    ]


def fetch_fixtures(args):
    if not shutil.which("git"):
        raise SystemExit("git is required to fetch fixture repositories")
    fixtures_dir = Path(args.fixtures_dir)
    repos = repo_manifest(fixtures_dir)
    if args.limit is not None:
        repos = repos[: args.limit]

    results = []
    for repo in repos:
        results.append(
            clone_or_update_repo(repo, repo_local_path(fixtures_dir, repo), update=args.update)
        )

    ok = all(item["status"] in {"already_present", "cloned", "updated"} for item in results)
    return {
        "schema": "m1nd-real-world-fixture-fetch-v0",
        "fixtures_dir": str(fixtures_dir),
        "ok": ok,
        "results": results,
        "non_claims": [
            "fetch-fixtures does not prove benchmark correctness",
            "external repos may change unless a round fixture lock captures the intended commits",
            "cloned repos are local fixtures and should not be committed into m1nd",
        ],
    }


def load_round_payload(path: Path):
    payload = load_json(path)
    if payload.get("schema") != ROUND_SCHEMA:
        raise SystemExit(f"{path} is not a {ROUND_SCHEMA} round")
    if not isinstance(payload.get("fixture_repos"), list):
        raise SystemExit(f"{path} missing fixture_repos[]")
    if not isinstance(payload.get("task_matrix"), list):
        raise SystemExit(f"{path} missing task_matrix[]")
    if not isinstance(payload.get("lanes"), list):
        raise SystemExit(f"{path} missing lanes[]")
    return payload


def load_fixture_lock(round_root: Path, round_payload):
    fixture_lock_ref = round_payload.get("fixture_lock_file")
    if not fixture_lock_ref:
        return {
            "schema": FIXTURE_LOCK_SCHEMA,
            "round_id": round_payload["round_id"],
            "fixture_repos": [fixture_lock_entry(repo) for repo in round_payload["fixture_repos"]],
        }, None

    path = resolve_path(fixture_lock_ref, round_root)
    payload = load_json(path)
    if payload.get("schema") != FIXTURE_LOCK_SCHEMA:
        raise SystemExit(f"{path} is not a {FIXTURE_LOCK_SCHEMA} lock")
    if payload.get("round_id") != round_payload.get("round_id"):
        raise SystemExit(f"{path} round_id does not match {round_payload['round_id']}")
    fixture_repos = payload.get("fixture_repos")
    if not isinstance(fixture_repos, list):
        raise SystemExit(f"{path} missing fixture_repos[]")
    for index, repo in enumerate(fixture_repos):
        if not isinstance(repo, dict):
            raise SystemExit(f"{path} fixture_repos[{index}] must be an object")
        if not repo.get("repo_id"):
            raise SystemExit(f"{path} fixture_repos[{index}].repo_id missing")
        lock_commit = repo.get("lock_commit")
        if lock_commit is not None and not isinstance(lock_commit, str):
            raise SystemExit(f"{path} fixture_repos[{index}].lock_commit must be a string or null")
    return payload, path


def prepare_lane_fixtures(args):
    if not shutil.which("git"):
        raise SystemExit("git is required to prepare lane fixture repositories")
    round_file = Path(args.round_file)
    round_payload = load_round_payload(round_file)
    round_root = round_file.resolve().parent
    fixture_lock, fixture_lock_path = load_fixture_lock(round_root, round_payload)
    fixture_lock_by_repo = {
        repo["repo_id"]: repo
        for repo in fixture_lock.get("fixture_repos", [])
        if isinstance(repo, dict) and repo.get("repo_id")
    }
    lane_workspace_root = Path(
        args.lane_fixtures_dir
        or round_payload.get("lane_workspace_root")
        or ".m1nd-benchmark-fixtures/real-world-lanes"
    )
    results = []
    for lane in round_payload.get("lanes", []):
        lane_results = []
        for repo in round_payload.get("fixture_repos", []):
            source_path = Path(repo["local_path"])
            destination = lane_workspace_root / lane["lane_id"] / repo["repo_id"]
            clone_result = clone_lane_repo(repo, source_path, destination, force=args.force)
            clone_result["seeded_artifacts"] = seed_lane_repo(repo, destination)
            lock_entry = fixture_lock_by_repo.get(repo["repo_id"], {})
            lock_commit = lock_entry.get("lock_commit")
            prepared_head_commit = git_head_commit(destination)
            clone_result["lock_commit"] = lock_commit
            clone_result["prepared_head_commit"] = prepared_head_commit
            clone_result["lock_check_enforced"] = bool(lock_commit)
            clone_result["lock_match"] = (
                prepared_head_commit == lock_commit if lock_commit else False
            )
            lane_results.append(clone_result)
        results.append(
            {
                "lane_id": lane["lane_id"],
                "arm": lane["arm"],
                "repos": lane_results,
            }
        )
    ok = all(
        repo["status"] in {"already_present", "prepared"}
        and (not repo.get("lock_check_enforced") or repo.get("lock_match") is True)
        for lane in results
        for repo in lane["repos"]
    )
    payload = {
        "schema": "m1nd-real-world-lane-fixtures-v0",
        "round_id": round_payload["round_id"],
        "lane_workspace_root": str(lane_workspace_root),
        "fixture_lock_file": str(fixture_lock_path) if fixture_lock_path else None,
        "ok": ok,
        "results": results,
        "non_claims": [
            "prepare-lane-fixtures does not run agents",
            "prepare-lane-fixtures does not prove task correctness",
            "fixture lock checks compare prepared HEAD commits to the operator-only round lock when a lock commit exists",
            "lane workspaces are local benchmark copies and should not be committed",
        ],
    }
    if args.write:
        dump_json(Path(args.write), payload)
    return payload


def validate_score(value, path):
    if not isinstance(value, int) or value < 0 or value > 4:
        raise SystemExit(f"{path} must be an integer from 0 to 4")
    return value


def load_lane_result(path: Path):
    payload = load_json(path)
    if payload.get("schema") != LANE_SCHEMA:
        raise SystemExit(f"{path} is not a {LANE_SCHEMA} result")
    if payload.get("arm") not in VALID_ARMS:
        raise SystemExit(f"{path} has unsupported arm {payload.get('arm')!r}")
    if not isinstance(payload.get("task_results"), list):
        raise SystemExit(f"{path} missing task_results[]")

    for index, task in enumerate(payload["task_results"]):
        if task.get("final_state") not in VALID_FINAL_STATES:
            raise SystemExit(f"{path} task_results[{index}].final_state invalid")
        if task.get("mode") not in VALID_TASK_MODES:
            raise SystemExit(f"{path} task_results[{index}].mode invalid")
        scores = task.get("scores")
        if not isinstance(scores, dict):
            raise SystemExit(f"{path} task_results[{index}].scores missing")
        for key in SCORE_KEYS:
            validate_score(scores.get(key), f"{path} task_results[{index}].scores.{key}")
        for list_key in (
            "files_opened",
            "tests_or_commands_run",
            "correct_files",
            "missed_files",
            "false_positive_files",
            "evidence",
            "event_refs",
        ):
            if not isinstance(task.get(list_key, []), list):
                raise SystemExit(f"{path} task_results[{index}].{list_key} must be a list")
    adjudications = payload.get("adjudications", [])
    if not isinstance(adjudications, list):
        raise SystemExit(f"{path} adjudications must be a list")
    for index, item in enumerate(adjudications):
        if not isinstance(item, dict):
            raise SystemExit(f"{path} adjudications[{index}] must be an object")
        if item.get("adjudicated_final_state") not in VALID_FINAL_STATES:
            raise SystemExit(f"{path} adjudications[{index}].adjudicated_final_state invalid")
        if item.get("comparability_class") not in VALID_COMPARABILITY_CLASSES:
            raise SystemExit(f"{path} adjudications[{index}].comparability_class invalid")
        scores = item.get("adjudicated_scores", {})
        if not isinstance(scores, dict):
            raise SystemExit(f"{path} adjudications[{index}].adjudicated_scores must be an object")
        for key in SCORE_KEYS:
            if key in scores:
                validate_score(scores.get(key), f"{path} adjudications[{index}].adjudicated_scores.{key}")
    return payload


def load_answer_key(path: Path, round_payload):
    payload = load_json(path)
    if payload.get("schema") != ANSWER_KEY_SCHEMA:
        raise SystemExit(f"{path} is not a {ANSWER_KEY_SCHEMA} answer key")
    if payload.get("round_id") != round_payload.get("round_id"):
        raise SystemExit(f"{path} round_id does not match {round_payload['round_id']}")
    tasks = payload.get("tasks")
    if not isinstance(tasks, list):
        raise SystemExit(f"{path} missing tasks[]")
    expected_task_ids = [pair["task"]["task_id"] for pair in round_payload["task_matrix"]]
    actual_task_ids = [item.get("task_id") for item in tasks]
    if actual_task_ids != expected_task_ids:
        raise SystemExit(f"{path} tasks do not match round task order")
    return payload


def task_score(task):
    return sum(int(task["scores"][key]) for key in SCORE_KEYS)


def empty_event_summary(path=None):
    return {
        "event_log_path": path,
        "event_log_exists": False,
        "event_count": 0,
        "agent_event_count": 0,
        "harness_event_count": 0,
        "invalid_event_count": 0,
        "event_type_counts": {},
        "m1nd_event_count": 0,
        "shell_event_count": 0,
        "test_event_count": 0,
    }


def load_event_stream(round_root: Path, lane):
    event_path = lane.get("event_log_path")
    summary = empty_event_summary(event_path)
    if not event_path:
        return summary
    path = Path(event_path)
    if not path.is_absolute():
        path = round_root / path
    summary["event_log_path"] = str(path)
    if not path.exists():
        return summary

    summary["event_log_exists"] = True
    event_type_counts = Counter()
    with path.open("r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                summary["invalid_event_count"] += 1
                continue
            if event.get("schema") != EVENT_SCHEMA:
                summary["invalid_event_count"] += 1
                continue
            event_type = event.get("event_type")
            if event_type not in VALID_EVENT_TYPES:
                summary["invalid_event_count"] += 1
                continue
            summary["event_count"] += 1
            event_type_counts[event_type] += 1
            if event.get("event_source") == "agent":
                summary["agent_event_count"] += 1
            elif event.get("event_source") == "harness":
                summary["harness_event_count"] += 1
            if event.get("m1nd_tool") or event_type == "m1nd_call":
                summary["m1nd_event_count"] += 1
            if event_type == "shell_command":
                summary["shell_event_count"] += 1
            if event_type == "test_run":
                summary["test_event_count"] += 1
    summary["event_type_counts"] = dict(sorted(event_type_counts.items()))
    return summary


def validate_lane_against_round(lane, round_payload, path):
    if lane.get("round_id") != round_payload.get("round_id"):
        raise SystemExit(f"{path} round_id does not match {round_payload['round_id']}")
    expected_pairs = round_payload.get("task_matrix", [])
    actual_tasks = lane.get("task_results", [])
    if len(actual_tasks) != len(expected_pairs):
        raise SystemExit(f"{path} task_results length does not match round task matrix")
    for index, pair in enumerate(expected_pairs):
        expected_task = pair["task"]
        expected_repo = pair["repo"]
        actual_task = actual_tasks[index]
        if actual_task.get("task_id") != expected_task.get("task_id"):
            raise SystemExit(f"{path} task_results[{index}].task_id does not match round task order")
        if actual_task.get("task_payload_id") != expected_task.get("payload_id"):
            raise SystemExit(f"{path} task_results[{index}].task_payload_id does not match round task order")
        if actual_task.get("repo_id") != expected_repo.get("repo_id"):
            raise SystemExit(f"{path} task_results[{index}].repo_id does not match round task matrix")
        if actual_task.get("mode") != expected_task.get("mode"):
            raise SystemExit(f"{path} task_results[{index}].mode does not match round task matrix")


def judge_task_result_summary(task):
    return {
        "task_id": task.get("task_id"),
        "task_payload_id": task.get("task_payload_id"),
        "repo_id": task.get("repo_id"),
        "mode": task.get("mode"),
        "final_state": task.get("final_state"),
        "scores": task.get("scores"),
        "time_to_good_context_ms": task.get("time_to_good_context_ms"),
        "time_to_full_proof_ms": task.get("time_to_full_proof_ms"),
        "false_start_count": task.get("false_start_count"),
        "search_iterations": task.get("search_iterations"),
        "files_opened": task.get("files_opened"),
        "tests_or_commands_run": task.get("tests_or_commands_run"),
        "code_changed": task.get("code_changed"),
        "requires_code_change": task.get("requires_code_change"),
        "patch_summary": task.get("patch_summary"),
        "correct_files": task.get("correct_files"),
        "missed_files": task.get("missed_files"),
        "false_positive_files": task.get("false_positive_files"),
        "claim_overreach": task.get("claim_overreach"),
        "primary_failure_class": task.get("primary_failure_class"),
        "notes": task.get("notes"),
        "evidence": task.get("evidence"),
        "event_refs": task.get("event_refs"),
        "agent_confidence": task.get("agent_confidence"),
    }


def judge_adjudication_templates(round_payload, primary_lanes):
    templates = []
    for lane in primary_lanes:
        for pair in round_payload.get("task_matrix", []):
            task = pair["task"]
            repo = pair["repo"]
            templates.append(
                {
                    "primary_lane_id": lane["lane_id"],
                    "primary_arm": lane["arm"],
                    "task_id": task["task_id"],
                    "task_payload_id": task.get("payload_id"),
                    "repo_id": repo["repo_id"],
                    "mode": task["mode"],
                    "adjudicated_final_state": None,
                    "adjudicated_scores": {key: None for key in SCORE_KEYS},
                    "comparability_class": None,
                    "exclusion_reason": "",
                    "notes": "",
                    "evidence": [],
                }
            )
    return templates


def lane_rollup(lane, event_summary=None):
    tasks = lane["task_results"]
    event_summary = event_summary or empty_event_summary(lane.get("event_log_path"))
    scored_tasks = [task_score(task) for task in tasks]
    evidence_count = sum(len(task.get("evidence") or []) for task in tasks)
    looks_like_template = (
        not lane.get("agent_testimony")
        and evidence_count == 0
        and sum(scored_tasks) == 0
        and event_summary["agent_event_count"] == 0
    )
    code_change_tasks = [task for task in tasks if task.get("requires_code_change")]
    code_change_completed = sum(
        1
        for task in code_change_tasks
        if task.get("code_changed") is True and task.get("final_state") in {"success", "partial"}
    )
    return {
        "lane_id": lane["lane_id"],
        "arm": lane["arm"],
        "looks_like_template": looks_like_template,
        "task_count": len(tasks),
        "success_count": sum(1 for task in tasks if task["final_state"] == "success"),
        "partial_count": sum(1 for task in tasks if task["final_state"] == "partial"),
        "failed_count": sum(1 for task in tasks if task["final_state"] == "failed"),
        "invalidated_count": sum(1 for task in tasks if task["final_state"] == "invalidated"),
        "run_score": sum(scored_tasks),
        "max_score": len(tasks) * len(SCORE_KEYS) * 4,
        "median_task_score": median(scored_tasks),
        "median_time_to_good_context_ms": median(
            task.get("time_to_good_context_ms") for task in tasks
        ),
        "median_time_to_full_proof_ms": median(
            task.get("time_to_full_proof_ms") for task in tasks
        ),
        "median_false_start_count": median(task.get("false_start_count", 0) for task in tasks),
        "files_opened": sum(len(task.get("files_opened") or []) for task in tasks),
        "search_iterations": sum(int(task.get("search_iterations") or 0) for task in tasks),
        "tests_or_commands_run": sum(len(task.get("tests_or_commands_run") or []) for task in tasks),
        "code_change_task_count": len(code_change_tasks),
        "code_change_completed_count": code_change_completed,
        "code_change_completion_rate": safe_rate(code_change_completed, len(code_change_tasks)),
        "claim_overreach_counts": dict(
            sorted(Counter(task.get("claim_overreach") or "none" for task in tasks).items())
        ),
        "failure_classes": dict(
            sorted(
                Counter(
                    task.get("primary_failure_class")
                    for task in tasks
                    if task.get("primary_failure_class")
                ).items()
            )
        ),
        "mode_success_counts": dict(
            sorted(
                Counter(
                    task.get("mode")
                    for task in tasks
                    if task.get("final_state") == "success"
                ).items()
            )
        ),
        "agent_testimony": lane.get("agent_testimony", ""),
        "event_log_path": event_summary["event_log_path"],
        "event_log_exists": event_summary["event_log_exists"],
        "event_count": event_summary["event_count"],
        "agent_event_count": event_summary["agent_event_count"],
        "invalid_event_count": event_summary["invalid_event_count"],
        "event_type_counts": event_summary["event_type_counts"],
        "m1nd_event_count": event_summary["m1nd_event_count"],
        "shell_event_count": event_summary["shell_event_count"],
        "test_event_count": event_summary["test_event_count"],
    }


def adjudication_score(item):
    scores = item.get("adjudicated_scores") or {}
    return sum(int(scores.get(key, 0)) for key in SCORE_KEYS)


def adjudication_rollup(lane_results):
    lane_by_id = {lane["lane_id"]: lane for lane in lane_results}
    adjudications = []
    for judge in lane_results:
        if judge.get("arm") != "adjudication":
            continue
        for item in judge.get("adjudications", []):
            primary_lane_id = item.get("primary_lane_id")
            primary_lane = lane_by_id.get(primary_lane_id, {})
            enriched = dict(item)
            enriched["judge_lane_id"] = judge["lane_id"]
            enriched["primary_arm"] = primary_lane.get("arm")
            enriched["adjudicated_score"] = adjudication_score(item)
            adjudications.append(enriched)

    by_arm = defaultdict(list)
    for item in adjudications:
        if item.get("primary_arm") in PRIMARY_ARMS:
            by_arm[item["primary_arm"]].append(item)

    arms = {}
    for arm, items in sorted(by_arm.items()):
        arms[arm] = {
            "adjudicated_task_count": len(items),
            "success_rate": safe_rate(
                sum(1 for item in items if item.get("adjudicated_final_state") == "success"),
                len(items),
            ),
            "excluded_rate": safe_rate(
                sum(1 for item in items if item.get("comparability_class") == "excluded"),
                len(items),
            ),
            "median_adjudicated_score": median(item["adjudicated_score"] for item in items),
            "comparability_classes": dict(
                sorted(Counter(item.get("comparability_class") for item in items).items())
            ),
        }

    primary_lane_ids = [
        lane["lane_id"] for lane in lane_results if lane.get("arm") in PRIMARY_ARMS
    ]
    adjudicated_primary_lane_ids = sorted(
        {
            item.get("primary_lane_id")
            for item in adjudications
            if item.get("primary_lane_id") in primary_lane_ids
        }
    )
    return {
        "schema": "m1nd-real-world-agent-adjudication-rollup-v0",
        "judge_lane_count": sum(1 for lane in lane_results if lane.get("arm") == "adjudication"),
        "adjudication_count": len(adjudications),
        "adjudicated_primary_lane_ids": adjudicated_primary_lane_ids,
        "missing_primary_adjudication_lanes": [
            lane_id for lane_id in primary_lane_ids if lane_id not in adjudicated_primary_lane_ids
        ],
        "complete_primary_adjudication": set(adjudicated_primary_lane_ids) == set(primary_lane_ids),
        "arms": arms,
        "items": adjudications,
    }


def score_round(args):
    runs_dir = Path(args.runs_dir)
    round_root = Path(args.round_root) if args.round_root else runs_dir.parent
    result_files = sorted(path for path in runs_dir.glob("*.json") if path.name != "report.json")
    lane_result_files = []
    ignored_result_files = []
    for path in result_files:
        try:
            payload = load_json(path)
        except json.JSONDecodeError as exc:
            raise SystemExit(f"{path} is not valid JSON: {exc}") from exc
        if isinstance(payload, dict) and payload.get("schema") == LANE_SCHEMA:
            lane_result_files.append(path)
        else:
            ignored_result_files.append(str(path))

    lane_results = [load_lane_result(path) for path in lane_result_files]
    event_summaries = {
        lane["lane_id"]: load_event_stream(round_root, lane)
        for lane in lane_results
    }
    rollups = [
        lane_rollup(lane, event_summaries.get(lane["lane_id"]))
        for lane in lane_results
    ]
    adjudication = adjudication_rollup(lane_results)

    by_arm = defaultdict(list)
    for rollup in rollups:
        by_arm[rollup["arm"]].append(rollup)

    arms = {}
    for arm, items in sorted(by_arm.items()):
        task_total = sum(item["task_count"] for item in items)
        success_total = sum(item["success_count"] for item in items)
        invalidated_total = sum(item["invalidated_count"] for item in items)
        arms[arm] = {
            "lanes": len(items),
            "task_count": task_total,
            "success_rate": safe_rate(success_total, task_total),
            "invalidated_rate": safe_rate(invalidated_total, task_total),
            "median_run_score": median(item["run_score"] for item in items),
            "median_time_to_good_context_ms": median(
                item["median_time_to_good_context_ms"] for item in items
            ),
            "median_time_to_full_proof_ms": median(
                item["median_time_to_full_proof_ms"] for item in items
            ),
            "median_false_start_count": median(item["median_false_start_count"] for item in items),
            "median_files_opened": median(item["files_opened"] for item in items),
            "median_search_iterations": median(item["search_iterations"] for item in items),
            "median_tests_or_commands_run": median(item["tests_or_commands_run"] for item in items),
            "median_agent_event_count": median(item["agent_event_count"] for item in items),
            "event_log_present_rate": safe_rate(
                sum(1 for item in items if item["event_log_exists"]), len(items)
            ),
            "agent_event_capture_rate": safe_rate(
                sum(1 for item in items if item["agent_event_count"] > 0), len(items)
            ),
            "invalid_event_count": sum(item["invalid_event_count"] for item in items),
            "code_change_task_count": sum(item["code_change_task_count"] for item in items),
            "code_change_completed_count": sum(item["code_change_completed_count"] for item in items),
            "code_change_completion_rate": safe_rate(
                sum(item["code_change_completed_count"] for item in items),
                sum(item["code_change_task_count"] for item in items),
            ),
            "claim_overreach_counts": dict(
                sorted(
                    sum(
                        (Counter(item["claim_overreach_counts"]) for item in items),
                        Counter(),
                    ).items()
                )
            ),
            "failure_classes": dict(
                sorted(
                    sum((Counter(item["failure_classes"]) for item in items), Counter()).items()
                )
            ),
            "mode_success_counts": dict(
                sorted(
                    sum((Counter(item["mode_success_counts"]) for item in items), Counter()).items()
                )
            ),
        }

    primary_lane_count_ok = (
        arms.get("m1nd_available", {}).get("lanes") == 3
        and arms.get("no_m1nd", {}).get("lanes") == 3
    )
    template_like_lanes = [item["lane_id"] for item in rollups if item["looks_like_template"]]
    primary_template_like_lanes = [
        item["lane_id"]
        for item in rollups
        if item["arm"] in PRIMARY_ARMS and item["looks_like_template"]
    ]
    comparable = primary_lane_count_ok and not primary_template_like_lanes and all(
        arms.get(arm, {}).get("task_count") == arms.get("m1nd_available", {}).get("task_count")
        for arm in PRIMARY_ARMS
    )
    event_capture_present = all(
        item["event_log_exists"] for item in rollups if item["arm"] in PRIMARY_ARMS
    )
    primary_agent_events_present = all(
        item["agent_event_count"] > 0 for item in rollups if item["arm"] in PRIMARY_ARMS
    )
    public_claim_blockers = [
        "single real-world benchmark round",
        "requires repeated comparable rounds across repo families",
    ]
    if not event_capture_present:
        public_claim_blockers.append("missing event logs for one or more primary lanes")
    if not primary_agent_events_present:
        public_claim_blockers.append("missing agent-authored event streams for one or more primary lanes")
    if not adjudication["complete_primary_adjudication"]:
        public_claim_blockers.append("missing complete adjudicated score layer for primary lanes")
        public_claim_blockers.append("requires adjudicated correctness for patch/review tasks")
    report = {
        "schema": REPORT_SCHEMA,
        "round_id": args.round_id or None,
        "generated_at": now_iso(),
        "runs_dir": str(runs_dir),
        "round_root": str(round_root),
        "lane_result_count": len(lane_results),
        "ignored_result_files": ignored_result_files,
        "primary_lane_count_ok": primary_lane_count_ok,
        "template_like_lanes": template_like_lanes,
        "primary_template_like_lanes": primary_template_like_lanes,
        "comparable_primary_arms": comparable,
        "event_capture_present": event_capture_present,
        "primary_agent_events_present": primary_agent_events_present,
        "public_claim_worthy": False,
        "public_claim_blockers": public_claim_blockers,
        "arms": arms,
        "adjudication": adjudication,
        "lanes": rollups,
        "non_claims": NON_CLAIMS,
    }
    if args.output:
        dump_json(Path(args.output), report)
    return report


def judge_input(args):
    round_file = Path(args.round_file)
    if not round_file.exists():
        raise SystemExit(f"{round_file} does not exist")
    lane_results_dir = Path(args.lane_results_dir)
    if not lane_results_dir.exists() or not lane_results_dir.is_dir():
        raise SystemExit(f"{lane_results_dir} is not a directory")
    answer_key_file = Path(args.answer_key)
    if not answer_key_file.exists():
        raise SystemExit(f"{answer_key_file} does not exist")

    round_payload = load_round_payload(round_file)
    answer_key = load_answer_key(answer_key_file, round_payload)
    round_root = round_file.resolve().parent
    primary_lanes = [
        lane
        for lane in round_payload.get("lanes", [])
        if lane.get("arm") in PRIMARY_ARMS
    ]
    if not primary_lanes:
        raise SystemExit(f"{round_file} has no primary lanes")

    event_summaries = {}
    primary_lane_results = []
    for lane in primary_lanes:
        lane_result_path = lane_results_dir / f"{lane['lane_id']}.json"
        if not lane_result_path.exists():
            raise SystemExit(f"missing primary lane result {lane_result_path}")
        lane_result = load_lane_result(lane_result_path)
        if lane_result.get("lane_id") != lane.get("lane_id"):
            raise SystemExit(f"{lane_result_path} lane_id does not match round lane")
        if lane_result.get("arm") != lane.get("arm"):
            raise SystemExit(f"{lane_result_path} arm does not match round lane")
        validate_lane_against_round(lane_result, round_payload, lane_result_path)
        event_summary = load_event_stream(round_root, lane_result)
        event_summaries[lane_result["lane_id"]] = event_summary
        primary_lane_results.append(
            {
                "lane_id": lane_result["lane_id"],
                "arm": lane_result["arm"],
                "model": lane_result.get("model", ""),
                "started_at": lane_result.get("started_at", ""),
                "finished_at": lane_result.get("finished_at", ""),
                "agent_testimony": lane_result.get("agent_testimony", ""),
                "lane_summary": lane_rollup(lane_result, event_summary),
                "task_results": [
                    judge_task_result_summary(task)
                    for task in lane_result.get("task_results", [])
                ],
            }
        )

    payload = {
        "schema": JUDGE_INPUT_SCHEMA,
        "round_id": round_payload["round_id"],
        "generated_at": now_iso(),
        "round_file": str(round_file),
        "lane_results_dir": str(lane_results_dir),
        "answer_key_file": str(answer_key_file),
        "task_matrix": round_payload["task_matrix"],
        "operator_answer_key": answer_key,
        "primary_lane_results": primary_lane_results,
        "event_summaries": event_summaries,
        "adjudication_templates": judge_adjudication_templates(round_payload, primary_lanes),
        "non_claims": [
            "judge-input is operator and adjudicator support data, not a primary lane prompt",
            "answer keys and adjudication templates do not replace direct review of task artifacts",
            "empty adjudication templates are scaffolds; judges still need to verify evidence and comparability",
        ],
    }
    if args.output:
        dump_json(Path(args.output), payload)
    return payload


def main():
    parser = argparse.ArgumentParser(description="Create and score real-world m1nd agent rounds.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    init_parser = subparsers.add_parser("init", help="Create a real-world benchmark round")
    init_parser.add_argument("--out-dir", required=True)
    init_parser.add_argument("--round-id")
    init_parser.add_argument("--fixtures-dir", default=".m1nd-benchmark-fixtures/real-world")
    init_parser.add_argument("--lane-fixtures-dir", default=".m1nd-benchmark-fixtures/real-world-lanes")
    init_parser.add_argument("--json", action="store_true")

    fetch_parser = subparsers.add_parser("fetch-fixtures", help="Clone or update external fixture repos")
    fetch_parser.add_argument("--fixtures-dir", default=".m1nd-benchmark-fixtures/real-world")
    fetch_parser.add_argument("--limit", type=int)
    fetch_parser.add_argument("--update", action="store_true")
    fetch_parser.add_argument("--json", action="store_true")

    prepare_parser = subparsers.add_parser(
        "prepare-lane-fixtures", help="Create isolated fixture checkouts for each lane"
    )
    prepare_parser.add_argument("--round-file", required=True)
    prepare_parser.add_argument("--lane-fixtures-dir")
    prepare_parser.add_argument("--write")
    prepare_parser.add_argument("--force", action="store_true")
    prepare_parser.add_argument("--json", action="store_true")

    score_parser = subparsers.add_parser("score", help="Score completed real-world lane results")
    score_parser.add_argument("--runs-dir", required=True)
    score_parser.add_argument("--output")
    score_parser.add_argument("--round-id")
    score_parser.add_argument("--round-root")
    score_parser.add_argument("--json", action="store_true")

    judge_parser = subparsers.add_parser(
        "judge-input", help="Build operator-only input for adjudicating primary lanes"
    )
    judge_parser.add_argument("--round-file", required=True)
    judge_parser.add_argument("--lane-results-dir", required=True)
    judge_parser.add_argument("--answer-key", required=True)
    judge_parser.add_argument("--output")
    judge_parser.add_argument("--json", action="store_true")

    args = parser.parse_args()
    if args.command == "init":
        result = init_round(args)
    elif args.command == "fetch-fixtures":
        result = fetch_fixtures(args)
    elif args.command == "prepare-lane-fixtures":
        result = prepare_lane_fixtures(args)
    elif args.command == "judge-input":
        result = judge_input(args)
    else:
        result = score_round(args)

    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
