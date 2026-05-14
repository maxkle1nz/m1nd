#!/usr/bin/env python3
"""Create and score blinded agent reliability benchmark rounds.

The harness is intentionally observational. It does not spawn agents and it does
not claim a public performance result by itself; it gives operators a stable
JSON contract for comparing m1nd-assisted and control lanes.
"""

import argparse
import json
import statistics
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path


ROUND_SCHEMA = "m1nd-agent-reliability-round-v0"
LANE_SCHEMA = "m1nd-agent-reliability-lane-result-v0"
REPORT_SCHEMA = "m1nd-agent-reliability-report-v0"

PRIMARY_ARMS = ("m1nd_available", "no_m1nd")
VALID_ARMS = (*PRIMARY_ARMS, "adjudication")
VALID_FINAL_STATES = ("success", "partial", "failed", "invalidated")
VALID_PROOF_MODES = ("unreported", "live", "static", "route_only", "mixed")
SCORE_KEYS = ("orientation", "recovery", "proof", "efficiency", "outcome")

NON_CLAIMS = [
    "no public performance claim is made from one benchmark round",
    "agent testimony is not evidence by itself without scored task results",
    "m1nd does not replace tests, compiler output, git history, rg, or direct file truth",
    "warm-graph results do not equal cold-start behavior",
    "host, transport, runtime, and workspace failures must be recorded instead of smoothed away",
]

DEFAULT_TASKS = [
    {
        "task_id": "multi_repo_orientation",
        "title": "Multi-repo orientation",
        "prompt": "Identify the correct repo, subsystem, and first files to inspect before proposing any action.",
        "requires_live_proof": False,
        "expected_evidence": [
            "correct repo named",
            "first relevant subsystem named",
            "file or module evidence cited",
        ],
    },
    {
        "task_id": "wrong_workspace_binding",
        "title": "Wrong workspace binding",
        "prompt": "Diagnose a likely wrong workspace or stale binding before trusting retrieval results.",
        "requires_live_proof": True,
        "expected_evidence": [
            "workspace mismatch named",
            "shortest honest recovery route named",
            "no false graph-health claim",
        ],
    },
    {
        "task_id": "transport_closed_recovery",
        "title": "Transport closed recovery",
        "prompt": "Recover or reroute after a dead MCP transport without fabricating success.",
        "requires_live_proof": True,
        "expected_evidence": [
            "transport failure detected",
            "fallback route recorded",
            "missing proof preserved",
        ],
    },
    {
        "task_id": "stale_runtime_route",
        "title": "Stale runtime or PATH route",
        "prompt": "Identify stale runtime, PATH shadowing, or tool-surface mismatch and name the repair command.",
        "requires_live_proof": True,
        "expected_evidence": [
            "runtime versions compared",
            "PATH or configured binary route checked",
            "host rebind caveat preserved",
        ],
    },
    {
        "task_id": "structural_edit_prep",
        "title": "Structural edit preparation",
        "prompt": "Gather enough connected context to name the safe edit target and focused proof steps.",
        "requires_live_proof": False,
        "expected_evidence": [
            "edit target named",
            "blast-radius or dependency reasoning cited",
            "proof gates named",
        ],
    },
    {
        "task_id": "root_cause_triage",
        "title": "Root-cause triage",
        "prompt": "From a realistic symptom, isolate the most likely fault boundary without broad file dumping.",
        "requires_live_proof": False,
        "expected_evidence": [
            "suspect boundary named",
            "alternative theory rejected or left open",
            "next verification command or file named",
        ],
    },
    {
        "task_id": "continuity_resume",
        "title": "Continuity resume",
        "prompt": "Continue a partially completed investigation without restarting from zero.",
        "requires_live_proof": True,
        "expected_evidence": [
            "prior state restored",
            "next unresolved question named",
            "unneeded rediscovery avoided",
        ],
    },
]


def now_iso():
    return datetime.now(timezone.utc).isoformat()


def dump_json(path: Path, payload):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, ensure_ascii=False)
        handle.write("\n")


def load_json(path: Path):
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


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


def lane_prompt(round_payload, lane):
    task_lines = []
    for task in round_payload["task_battery"]:
        evidence = ", ".join(task["expected_evidence"])
        task_lines.append(
            f"- {task['task_id']}: {task['prompt']} Expected evidence: {evidence}."
        )

    if lane["arm"] == "m1nd_available":
        tool_rule = (
            "Use m1nd as the first investigative layer when available. If m1nd is "
            "blocked, stale, or bound to the wrong workspace, record that recovery "
            "path and fall back honestly."
        )
    elif lane["arm"] == "no_m1nd":
        tool_rule = (
            "Do not use m1nd. Use normal repo investigation tools such as rg, file "
            "reads, git, tests, and compiler output."
        )
    else:
        tool_rule = (
            "Adjudicate one disputed or failed task after primary lanes finish. Do "
            "not change primary scores; explain the pass/fail rationale."
        )

    return "\n".join(
        [
            f"# Agent Reliability Lane: {lane['lane_id']}",
            "",
            f"Round: `{round_payload['round_id']}`",
            f"Arm: `{lane['arm']}`",
            f"Repo: `{round_payload['repo']}`",
            "",
            tool_rule,
            "",
            "Do not guess the benchmark hypothesis. Work as if this is normal operator work.",
            "Record failures, missing proof, and host/runtime anomalies instead of hiding them.",
            "",
            "## Task Battery",
            "",
            *task_lines,
            "",
            "## Required Result",
            "",
            "Fill a JSON result using the template in `lane-result-template.json`.",
        ]
    )


def result_template(round_payload, lane):
    return {
        "schema": LANE_SCHEMA,
        "round_id": round_payload["round_id"],
        "lane_id": lane["lane_id"],
        "arm": lane["arm"],
        "repo": round_payload["repo"],
        "model": "",
        "started_at": "",
        "finished_at": "",
        "agent_testimony": "",
        "task_results": [
            {
                "task_id": task["task_id"],
                "final_state": "partial",
                "scores": {
                    "orientation": 0,
                    "recovery": 0,
                    "proof": 0,
                    "efficiency": 0,
                    "outcome": 0,
                },
                "time_to_good_context_ms": None,
                "time_to_full_proof_ms": None,
                "requires_live_proof": bool(task.get("requires_live_proof")),
                "proof_mode": "unreported",
                "live_state_verified": False,
                "evidence_origin": [],
                "raw_event_evidence": [],
                "false_start_count": 0,
                "files_opened": [],
                "search_iterations": 0,
                "recovery_events": [],
                "recovery_followed": False,
                "wrong_workspace_detected": False,
                "wrong_workspace_recovered": False,
                "transport_failure_detected": False,
                "transport_failure_recovered": False,
                "stale_path_detected": False,
                "stale_path_recovered": False,
                "claim_overreach": "none",
                "primary_failure_class": None,
                "notes": "",
                "evidence": [],
            }
            for task in round_payload["task_battery"]
        ],
    }


def init_round(args):
    out_dir = Path(args.out_dir)
    round_id = args.round_id or f"agent-usefulness-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}"
    round_payload = {
        "schema": ROUND_SCHEMA,
        "round_id": round_id,
        "created_at": now_iso(),
        "repo": str(Path(args.repo).resolve()),
        "condition": args.condition,
        "status": "planned",
        "lane_count": 7,
        "lanes": lane_plan(),
        "task_battery": DEFAULT_TASKS,
        "success_criteria": [
            "lane reaches the correct target or honest recovery route",
            "lane does not claim proof it did not gather",
            "lane preserves missing proof when proof is missing",
            "lane avoids unnecessary restart behavior after useful hints",
        ],
        "non_claims": NON_CLAIMS,
    }

    dump_json(out_dir / "round.json", round_payload)
    dump_json(out_dir / "lane-result-template.json", result_template(round_payload, round_payload["lanes"][0]))
    for lane in round_payload["lanes"]:
        prompt_path = out_dir / "lane-prompts" / f"{lane['lane_id']}.md"
        prompt_path.parent.mkdir(parents=True, exist_ok=True)
        prompt_path.write_text(lane_prompt(round_payload, lane) + "\n", encoding="utf-8")
        dump_json(out_dir / "lane-results" / f"{lane['lane_id']}.json", result_template(round_payload, lane))

    return {
        "schema": ROUND_SCHEMA,
        "round_id": round_id,
        "out_dir": str(out_dir),
        "round_file": str(out_dir / "round.json"),
        "lane_prompts_dir": str(out_dir / "lane-prompts"),
        "lane_results_dir": str(out_dir / "lane-results"),
        "lane_count": len(round_payload["lanes"]),
        "task_count": len(round_payload["task_battery"]),
        "non_claims": NON_CLAIMS,
    }


def validate_score(value, path):
    if not isinstance(value, int) or value < 0 or value > 4:
        raise SystemExit(f"{path} must be an integer from 0 to 4")
    return value


def default_requires_live_proof(task_id):
    for task in DEFAULT_TASKS:
        if task["task_id"] == task_id:
            return bool(task.get("requires_live_proof"))
    return False


def load_lane_result(path: Path):
    payload = load_json(path)
    if payload.get("schema") != LANE_SCHEMA:
        raise SystemExit(f"{path} is not a {LANE_SCHEMA} result")
    if payload.get("arm") not in VALID_ARMS:
        raise SystemExit(f"{path} has unsupported arm {payload.get('arm')!r}")
    if not isinstance(payload.get("task_results"), list):
        raise SystemExit(f"{path} missing task_results[]")

    for index, task in enumerate(payload["task_results"]):
        final_state = task.get("final_state")
        if final_state not in VALID_FINAL_STATES:
            raise SystemExit(f"{path} task_results[{index}].final_state invalid: {final_state!r}")
        scores = task.get("scores")
        if not isinstance(scores, dict):
            raise SystemExit(f"{path} task_results[{index}].scores missing")
        for key in SCORE_KEYS:
            validate_score(scores.get(key), f"{path} task_results[{index}].scores.{key}")
        proof_mode = task.get("proof_mode", "unreported")
        if proof_mode not in VALID_PROOF_MODES:
            raise SystemExit(
                f"{path} task_results[{index}].proof_mode invalid: {proof_mode!r}"
            )
        evidence_origin = task.get("evidence_origin", [])
        if not isinstance(evidence_origin, list):
            raise SystemExit(f"{path} task_results[{index}].evidence_origin must be a list")
        raw_event_evidence = task.get("raw_event_evidence", [])
        if not isinstance(raw_event_evidence, list):
            raise SystemExit(f"{path} task_results[{index}].raw_event_evidence must be a list")
    return payload


def task_score(task):
    return sum(int(task["scores"][key]) for key in SCORE_KEYS)


def lane_rollup(lane):
    tasks = lane["task_results"]
    scored_tasks = [task_score(task) for task in tasks]
    success_count = sum(1 for task in tasks if task["final_state"] == "success")
    invalidated_count = sum(1 for task in tasks if task["final_state"] == "invalidated")
    recovery_events = sum(len(task.get("recovery_events") or []) for task in tasks)
    recovery_followed = sum(1 for task in tasks if task.get("recovery_followed") is True)
    recovery_opportunities = sum(
        1
        for task in tasks
        if task.get("recovery_followed") is True or len(task.get("recovery_events") or []) > 0
    )
    files_opened = sum(len(task.get("files_opened") or []) for task in tasks)
    evidence_count = sum(len(task.get("evidence") or []) for task in tasks)
    raw_event_evidence_count = sum(len(task.get("raw_event_evidence") or []) for task in tasks)
    live_required_count = sum(
        1
        for task in tasks
        if bool(task.get("requires_live_proof", default_requires_live_proof(task.get("task_id"))))
    )
    live_verified_count = sum(1 for task in tasks if task.get("live_state_verified") is True)
    live_required_verified_count = sum(
        1
        for task in tasks
        if bool(task.get("requires_live_proof", default_requires_live_proof(task.get("task_id"))))
        and task.get("live_state_verified") is True
    )
    live_proof_gap_count = sum(
        1
        for task in tasks
        if bool(task.get("requires_live_proof", default_requires_live_proof(task.get("task_id"))))
        and task.get("live_state_verified") is not True
    )
    live_required_success_without_live_count = sum(
        1
        for task in tasks
        if bool(task.get("requires_live_proof", default_requires_live_proof(task.get("task_id"))))
        and task.get("live_state_verified") is not True
        and task.get("final_state") == "success"
    )
    live_required_score_cap_violation_count = sum(
        1
        for task in tasks
        if bool(task.get("requires_live_proof", default_requires_live_proof(task.get("task_id"))))
        and task.get("live_state_verified") is not True
        and (
            task.get("final_state") == "success"
            or int(task.get("scores", {}).get("proof", 0)) > 2
            or int(task.get("scores", {}).get("outcome", 0)) > 2
        )
    )
    proof_mode_counts = dict(
        sorted(Counter(task.get("proof_mode", "unreported") for task in tasks).items())
    )
    evidence_origin_counts = dict(
        sorted(
            Counter(
                origin
                for task in tasks
                for origin in (task.get("evidence_origin") or [])
            ).items()
        )
    )
    looks_like_template = (
        not lane.get("agent_testimony")
        and evidence_count == 0
        and sum(scored_tasks) == 0
    )
    return {
        "lane_id": lane["lane_id"],
        "arm": lane["arm"],
        "looks_like_template": looks_like_template,
        "task_count": len(tasks),
        "success_count": success_count,
        "partial_count": sum(1 for task in tasks if task["final_state"] == "partial"),
        "failed_count": sum(1 for task in tasks if task["final_state"] == "failed"),
        "invalidated_count": invalidated_count,
        "run_score": sum(scored_tasks),
        "max_score": len(tasks) * len(SCORE_KEYS) * 4,
        "median_task_score": median(scored_tasks),
        "median_time_to_good_context_ms": median(
            task.get("time_to_good_context_ms") for task in tasks
        ),
        "median_false_start_count": median(task.get("false_start_count", 0) for task in tasks),
        "search_iterations": sum(int(task.get("search_iterations") or 0) for task in tasks),
        "files_opened": files_opened,
        "evidence_count": evidence_count,
        "raw_event_evidence_count": raw_event_evidence_count,
        "proof_mode_counts": proof_mode_counts,
        "evidence_origin_counts": evidence_origin_counts,
        "live_required_count": live_required_count,
        "live_verified_count": live_verified_count,
        "live_required_verified_count": live_required_verified_count,
        "live_proof_gap_count": live_proof_gap_count,
        "live_required_success_without_live_count": live_required_success_without_live_count,
        "live_required_score_cap_violation_count": live_required_score_cap_violation_count,
        "live_required_verified_rate": safe_rate(
            live_required_verified_count, live_required_count
        ),
        "recovery_events": recovery_events,
        "recovery_followed": recovery_followed,
        "recovery_opportunities": recovery_opportunities,
        "recovery_followed_rate": safe_rate(recovery_followed, recovery_opportunities),
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
        "agent_testimony": lane.get("agent_testimony", ""),
    }


def score_round(args):
    runs_dir = Path(args.runs_dir)
    result_files = sorted(
        path
        for path in runs_dir.glob("*.json")
        if path.name not in {"round.json", "lane-result-template.json", "report.json"}
    )
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
    lane_rollups = [lane_rollup(lane) for lane in lane_results]

    by_arm = defaultdict(list)
    for rollup in lane_rollups:
        by_arm[rollup["arm"]].append(rollup)

    arms = {}
    for arm, rollups in sorted(by_arm.items()):
        task_total = sum(item["task_count"] for item in rollups)
        success_total = sum(item["success_count"] for item in rollups)
        invalidated_total = sum(item["invalidated_count"] for item in rollups)
        recovery_events = sum(item["recovery_events"] for item in rollups)
        recovery_followed = sum(item["recovery_followed"] for item in rollups)
        recovery_opportunities = sum(item["recovery_opportunities"] for item in rollups)
        live_required_count = sum(item["live_required_count"] for item in rollups)
        live_required_verified_count = sum(
            item["live_required_verified_count"] for item in rollups
        )
        live_proof_gap_count = sum(item["live_proof_gap_count"] for item in rollups)
        live_required_success_without_live_count = sum(
            item["live_required_success_without_live_count"] for item in rollups
        )
        live_required_score_cap_violation_count = sum(
            item["live_required_score_cap_violation_count"] for item in rollups
        )
        arms[arm] = {
            "lanes": len(rollups),
            "task_count": task_total,
            "success_rate": safe_rate(success_total, task_total),
            "invalidated_rate": safe_rate(invalidated_total, task_total),
            "median_run_score": median(item["run_score"] for item in rollups),
            "median_time_to_good_context_ms": median(
                item["median_time_to_good_context_ms"] for item in rollups
            ),
            "median_false_start_count": median(
                item["median_false_start_count"] for item in rollups
            ),
            "median_files_opened": median(item["files_opened"] for item in rollups),
            "median_search_iterations": median(item["search_iterations"] for item in rollups),
            "live_required_count": live_required_count,
            "live_required_verified_count": live_required_verified_count,
            "live_required_verified_rate": safe_rate(
                live_required_verified_count, live_required_count
            ),
            "live_proof_gap_count": live_proof_gap_count,
            "live_required_success_without_live_count": live_required_success_without_live_count,
            "live_required_score_cap_violation_count": live_required_score_cap_violation_count,
            "proof_mode_counts": dict(
                sorted(
                    sum((Counter(item["proof_mode_counts"]) for item in rollups), Counter()).items()
                )
            ),
            "evidence_origin_counts": dict(
                sorted(
                    sum(
                        (Counter(item["evidence_origin_counts"]) for item in rollups),
                        Counter(),
                    ).items()
                )
            ),
            "recovery_followed_rate": safe_rate(recovery_followed, recovery_opportunities),
            "failure_classes": dict(
                sorted(
                    sum((Counter(item["failure_classes"]) for item in rollups), Counter()).items()
                )
            ),
            "claim_overreach_counts": dict(
                sorted(
                    sum(
                        (Counter(item["claim_overreach_counts"]) for item in rollups),
                        Counter(),
                    ).items()
                )
            ),
        }

    lane_count_ok = (
        arms.get("m1nd_available", {}).get("lanes") == 3
        and arms.get("no_m1nd", {}).get("lanes") == 3
    )
    template_like_lanes = [
        rollup["lane_id"] for rollup in lane_rollups if rollup["looks_like_template"]
    ]
    primary_template_like_lanes = [
        rollup["lane_id"]
        for rollup in lane_rollups
        if rollup["arm"] in PRIMARY_ARMS and rollup["looks_like_template"]
    ]
    comparable = lane_count_ok and all(
        arms.get(arm, {}).get("task_count") == arms.get("m1nd_available", {}).get("task_count")
        for arm in PRIMARY_ARMS
    ) and not primary_template_like_lanes
    live_proof_comparable = comparable and all(
        arms.get(arm, {}).get("live_required_score_cap_violation_count") == 0
        for arm in PRIMARY_ARMS
    )
    comparability_blockers = []
    if primary_template_like_lanes:
        comparability_blockers.append("primary lane result still looks like a template")
    if not lane_count_ok:
        comparability_blockers.append("primary lane count mismatch")
    if comparable and not live_proof_comparable:
        comparability_blockers.append(
            "at least one live-required task was scored as success or high proof without live_state_verified"
        )

    report = {
        "schema": REPORT_SCHEMA,
        "round_id": args.round_id or None,
        "generated_at": now_iso(),
        "runs_dir": str(runs_dir),
        "lane_result_count": len(lane_results),
        "ignored_result_files": ignored_result_files,
        "primary_lane_count_ok": lane_count_ok,
        "template_like_lanes": template_like_lanes,
        "primary_template_like_lanes": primary_template_like_lanes,
        "structurally_comparable_primary_arms": comparable,
        "live_proof_comparable_primary_arms": live_proof_comparable,
        "comparable_primary_arms": live_proof_comparable,
        "comparability_blockers": comparability_blockers,
        "public_claim_worthy": False,
        "public_claim_blockers": [
            "single benchmark round",
            "requires repeated comparable rounds before headline claims",
            "live-proof tasks must be separated from route-only or static inference",
        ],
        "arms": arms,
        "lanes": lane_rollups,
        "non_claims": NON_CLAIMS,
    }
    if args.output:
        dump_json(Path(args.output), report)
    return report


def main():
    parser = argparse.ArgumentParser(description="Create and score m1nd agent reliability rounds.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    init_parser = subparsers.add_parser("init", help="Create a blinded benchmark round directory")
    init_parser.add_argument("--out-dir", required=True)
    init_parser.add_argument("--round-id")
    init_parser.add_argument("--repo", default=".")
    init_parser.add_argument(
        "--condition",
        default="mixed",
        choices=["mixed", "cold", "warm", "host-recovery"],
    )
    init_parser.add_argument("--json", action="store_true")

    score_parser = subparsers.add_parser("score", help="Score lane result JSON files")
    score_parser.add_argument("--runs-dir", required=True)
    score_parser.add_argument("--output")
    score_parser.add_argument("--round-id")
    score_parser.add_argument("--json", action="store_true")

    args = parser.parse_args()
    if args.command == "init":
        result = init_round(args)
    else:
        result = score_round(args)

    if args.json:
        print(json.dumps(result, indent=2, ensure_ascii=False))
    else:
        print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
