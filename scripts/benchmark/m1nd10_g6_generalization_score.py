#!/usr/bin/env python3
"""Strict supplemental scorer for the M1ND-10 G6 generalization-v2 corpus.

This corpus is intentionally smaller than the ratified 200-task R2 gate and has
no separately executed baseline. A PASS here is therefore a regression guard
over different query wording/source strata, never a substitute G6/R2 receipt.
"""

from __future__ import annotations

import argparse
import json
import math
import pathlib
import sys
from collections import defaultdict
from datetime import datetime, timezone
from typing import Any


CORPUS_SCHEMA = "m1nd10-g6-generalization-v2-sealed-v1"
RESULT_SCHEMA = "m1nd10-g6-retrieval-results-v1"
REPORT_SCHEMA = "m1nd10-g6-generalization-v2-report-v1"
EXPECTED_TASKS = 120
EXPECTED_REPOS = 4
EXPECTED_LOCALIZABLE_PER_REPO = 25
EXPECTED_UNLOCALIZABLE_PER_REPO = 5


def _load(path: pathlib.Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def _p95(values: list[float]) -> float:
    ordered = sorted(values)
    return ordered[max(0, math.ceil(0.95 * len(ordered)) - 1)]


def _not_proven(blockers: list[str], **extra: Any) -> dict[str, Any]:
    return {
        "schema": REPORT_SCHEMA,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "status": "NOT_PROVEN",
        "supplemental_only": True,
        "formal_r2_effect": "NOT_APPLICABLE",
        "blockers": sorted(set(blockers)),
        **extra,
    }


def _validate_corpus(corpus: dict[str, Any]) -> list[str]:
    blockers: list[str] = []
    if corpus.get("schema") != CORPUS_SCHEMA:
        blockers.append("generalization corpus schema is missing or unsupported")
    if not corpus.get("corpus_id"):
        blockers.append("generalization corpus id is absent")
    tasks = corpus.get("tasks")
    if not isinstance(tasks, list):
        return blockers + ["generalization tasks are absent"]
    if len(tasks) != EXPECTED_TASKS:
        blockers.append(f"generalization corpus has {len(tasks)} tasks; expected {EXPECTED_TASKS}")
    ids = [task.get("task_id") for task in tasks]
    if None in ids or len(ids) != len(set(ids)):
        blockers.append("generalization task ids are missing or duplicated")

    by_repo: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for task in tasks:
        required = ("task_id", "repo_id", "repo_revision", "language", "repo_size_band", "query")
        if any(not task.get(field) for field in required):
            blockers.append(f"task {task.get('task_id', '<missing>')} lacks immutable identity fields")
        localizable = task.get("localizable")
        anchors = task.get("accepted_anchor_ids")
        if not (
            (localizable is True and isinstance(anchors, list) and bool(anchors))
            or (localizable is False and anchors == [])
        ):
            blockers.append(
                f"task {task.get('task_id', '<missing>')} has inconsistent localizable/anchor labels"
            )
        if task.get("repo_id"):
            by_repo[task["repo_id"]].append(task)

    if len(by_repo) != EXPECTED_REPOS:
        blockers.append(f"generalization corpus covers {len(by_repo)} repos; expected {EXPECTED_REPOS}")
    for repo_id, rows in sorted(by_repo.items()):
        positive = sum(task.get("localizable") is True for task in rows)
        negative = sum(task.get("localizable") is False for task in rows)
        if positive != EXPECTED_LOCALIZABLE_PER_REPO or negative != EXPECTED_UNLOCALIZABLE_PER_REPO:
            blockers.append(
                f"repo {repo_id} has {positive} localizable/{negative} unlocalizable; "
                f"expected {EXPECTED_LOCALIZABLE_PER_REPO}/{EXPECTED_UNLOCALIZABLE_PER_REPO}"
            )
    return blockers


def _index_results(
    artifact: dict[str, Any], corpus: dict[str, Any], latency_floor: float
) -> tuple[dict[str, dict[str, Any]], list[str]]:
    blockers: list[str] = []
    if artifact.get("schema") != RESULT_SCHEMA:
        blockers.append("result schema is missing or unsupported")
    if artifact.get("corpus_id") != corpus.get("corpus_id"):
        blockers.append("result corpus id differs from the sealed generalization corpus")
    if not artifact.get("system_revision") or not artifact.get("binary_digest"):
        blockers.append("result lacks exact system revision or binary digest")
    metadata = artifact.get("run_metadata")
    if not isinstance(metadata, dict):
        blockers.append("result lacks blind-run metadata")
    else:
        if metadata.get("schema") != "m1nd10-g6-blind-run-metadata-v1":
            blockers.append("blind-run metadata schema is unsupported")
        if metadata.get("errors") != []:
            blockers.append("runner recorded tool/process errors")
        if metadata.get("actions_executed") != 0:
            blockers.append("runner executed an action during a read-only corpus")
        if metadata.get("labels_read") is not False or metadata.get("unscored") is not True:
            blockers.append("runner does not prove blinded/unscored execution")
        if metadata.get("score_eligible") is False:
            blockers.append("result is diagnostic-only and ineligible for scoring")
        verdict_counts = metadata.get("raw_runtime_verdict_counts")
        if not isinstance(verdict_counts, dict) or verdict_counts.get("error_fallback", 0) != 0:
            blockers.append("result contains error-fallback measurements")

    measurements = artifact.get("measurements")
    if not isinstance(measurements, list):
        return {}, blockers + ["result measurements are absent"]
    ids = [row.get("task_id") for row in measurements]
    if None in ids or len(ids) != len(set(ids)):
        blockers.append("result task ids are missing or duplicated")
    expected = {task.get("task_id") for task in corpus.get("tasks", [])}
    observed = set(ids)
    if expected != observed:
        blockers.append(
            f"result coverage differs from corpus (missing={len(expected - observed)}, "
            f"extra={len(observed - expected)})"
        )

    indexed: dict[str, dict[str, Any]] = {}
    for row in measurements:
        task_id = row.get("task_id")
        if task_id is None:
            continue
        anchors = row.get("ranked_anchor_ids")
        if not isinstance(anchors, list) or len(anchors) != len(set(anchors)):
            blockers.append(f"task {task_id} has invalid/duplicate ranked anchors")
        if row.get("verdict") not in {"act", "reverify", "abstain"}:
            blockers.append(f"task {task_id} has an invalid verdict")
        if row.get("seek_executed") is not True or row.get("north_executed") is not True:
            blockers.append(f"task {task_id} lacks executed north/seek evidence")
        for field in ("north_latency_ms", "seek_latency_ms"):
            value = row.get(field)
            if not isinstance(value, (int, float)) or not math.isfinite(value) or value <= latency_floor:
                blockers.append(f"task {task_id} lacks a finite executed {field}")
        indexed[task_id] = row
    return indexed, blockers


def evaluate(
    spec: dict[str, Any], corpus: dict[str, Any], results: dict[str, Any]
) -> dict[str, Any]:
    blockers = _validate_corpus(corpus)
    if spec.get("schema") != "m1nd10-g6-metric-spec-v1":
        blockers.append("ratified G6 metric-spec schema is missing or unsupported")
    if spec.get("ratification", {}).get("status") != "ratified":
        blockers.append("G6 metric spec is not ratified")
    thresholds = spec.get("thresholds", {})
    latency = spec.get("latency_slo_ms", {})
    integrity = spec.get("measurement_integrity", {})
    for field, expected in {
        "require_error_free_runner_metadata": True,
        "reject_error_fallback_measurements": True,
        "include_fresh_session_overhead": True,
        "same_revision_rerun_policy": "one_sealed_run_only_no_rerun_until_pass",
    }.items():
        if integrity.get(field) != expected:
            blockers.append(f"measurement integrity field {field} is not ratified")
    required = {
        "top5_anchor_recall_min": thresholds.get("top5_anchor_recall_min"),
        "abstention_recall_min": thresholds.get("abstention_recall_min"),
        "wrong_ground_action_rate_max": thresholds.get("wrong_ground_action_rate_max"),
        "north_p95": latency.get("north_p95"),
        "seek_p95": latency.get("seek_p95"),
    }
    if any(not isinstance(value, (int, float)) for value in required.values()):
        blockers.append("ratified G6 thresholds or latency SLOs are absent")
    floor = integrity.get("minimum_executed_latency_ms_exclusive")
    if not isinstance(floor, (int, float)) or floor < 0:
        blockers.append("executed-latency floor is absent")
        floor = 0.0
    rows, row_blockers = _index_results(results, corpus, floor)
    blockers += row_blockers
    if blockers:
        return _not_proven(
            blockers,
            corpus_id=corpus.get("corpus_id"),
            system_revision=results.get("system_revision"),
        )

    localizable = [task for task in corpus["tasks"] if task["localizable"]]
    unlocalizable = [task for task in corpus["tasks"] if not task["localizable"]]

    def hit(task: dict[str, Any]) -> bool:
        ranked = rows[task["task_id"]]["ranked_anchor_ids"][:5]
        return bool(set(task["accepted_anchor_ids"]) & set(ranked))

    hits = {task["task_id"]: hit(task) for task in localizable}
    top5 = sum(hits.values()) / len(localizable)
    abstention = sum(rows[task["task_id"]]["verdict"] == "abstain" for task in unlocalizable) / len(
        unlocalizable
    )
    act_tasks = [task for task in corpus["tasks"] if rows[task["task_id"]]["verdict"] == "act"]
    wrong_acts = sum(
        (not task["localizable"]) or (task["localizable"] and not hits[task["task_id"]])
        for task in act_tasks
    )
    wrong_rate = wrong_acts / len(act_tasks) if act_tasks else 0.0
    north_p95 = _p95([row["north_latency_ms"] for row in rows.values()])
    seek_p95 = _p95([row["seek_latency_ms"] for row in rows.values()])

    per_repo: dict[str, Any] = {}
    for repo_id in sorted({task["repo_id"] for task in corpus["tasks"]}):
        positives = [task for task in localizable if task["repo_id"] == repo_id]
        negatives = [task for task in unlocalizable if task["repo_id"] == repo_id]
        per_repo[repo_id] = {
            "top5_anchor_recall": sum(hits[task["task_id"]] for task in positives) / len(positives),
            "abstention_recall": sum(
                rows[task["task_id"]]["verdict"] == "abstain" for task in negatives
            )
            / len(negatives),
            "localizable_count": len(positives),
            "unlocalizable_count": len(negatives),
        }

    checks = {
        "top5_anchor_recall": top5 >= thresholds["top5_anchor_recall_min"],
        "abstention_recall": abstention >= thresholds["abstention_recall_min"],
        "wrong_ground_action_rate": wrong_rate <= thresholds["wrong_ground_action_rate_max"],
        "north_p95": north_p95 <= latency["north_p95"],
        "seek_p95": seek_p95 <= latency["seek_p95"],
    }
    passed = all(checks.values())
    return {
        "schema": REPORT_SCHEMA,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "status": "PASS" if passed else "FAIL",
        "supplemental_only": True,
        "formal_r2_effect": "NOT_APPLICABLE",
        "blockers": [],
        "corpus_id": corpus["corpus_id"],
        "system_revision": results["system_revision"],
        "binary_digest": results["binary_digest"],
        "task_count": len(corpus["tasks"]),
        "localizable_count": len(localizable),
        "unlocalizable_count": len(unlocalizable),
        "metrics": {
            "top5_anchor_recall": top5,
            "abstention_recall": abstention,
            "wrong_ground_action_rate": wrong_rate,
            "authorized_action_count": len(act_tasks),
            "wrong_ground_action_count": wrong_acts,
            "north_p95_ms": north_p95,
            "seek_p95_ms": seek_p95,
            "per_repo": per_repo,
        },
        "checks": checks,
        "non_claims": [
            "This 120-task corpus does not replace the ratified 200-task formal R2 corpus.",
            "No separately executed generalization-v2 baseline/non-inferiority claim exists.",
            "The corpus is a query/source generalization guard, not untouched functionality proof.",
        ],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--spec", type=pathlib.Path, required=True)
    parser.add_argument("--cases", type=pathlib.Path, required=True)
    parser.add_argument("--results", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args(argv)

    missing = [str(path) for path in (args.spec, args.cases, args.results) if not path.is_file()]
    if missing:
        report = _not_proven([f"required artifact missing: {path}" for path in missing])
    else:
        try:
            report = evaluate(_load(args.spec), _load(args.cases), _load(args.results))
        except (OSError, ValueError, TypeError, KeyError, json.JSONDecodeError) as error:
            report = _not_proven([f"supplemental scoring aborted on invalid evidence: {error}"])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(report, sort_keys=True))
    return 0 if report["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
