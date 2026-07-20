from __future__ import annotations

import copy
import importlib.util
import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "benchmark" / "m1nd10_g6_generalization_score.py"
SPEC = json.loads((ROOT / "docs" / "benchmarks" / "m1nd10-g6-metric-spec-v1.json").read_text())
try:
    CORPUS = json.loads(
        (
            ROOT / "docs" / "benchmarks" / "m1nd10-g6-generalization-v2" / "operator-only" / "corpus.json"
        ).read_text()
    )
except FileNotFoundError as error:
    # Operator-only material is deliberately absent from public checkouts and
    # candidate trees; this suite is operator-local by design.
    raise unittest.SkipTest("operator-only generalization corpus not present") from error

module_spec = importlib.util.spec_from_file_location("m1nd10_g6_generalization_score", MODULE_PATH)
assert module_spec and module_spec.loader
score = importlib.util.module_from_spec(module_spec)
module_spec.loader.exec_module(score)


def result_for(corpus: dict) -> dict:
    measurements = []
    for task in corpus["tasks"]:
        measurements.append(
            {
                "task_id": task["task_id"],
                "ranked_anchor_ids": task["accepted_anchor_ids"][:1],
                "verdict": "reverify" if task["localizable"] else "abstain",
                "north_latency_ms": 10.0,
                "seek_latency_ms": 5.0,
                "north_executed": True,
                "seek_executed": True,
            }
        )
    return {
        "schema": score.RESULT_SCHEMA,
        "corpus_id": corpus["corpus_id"],
        "system_revision": "test-revision",
        "binary_digest": "sha256:" + "a" * 64,
        "run_metadata": {
            "schema": "m1nd10-g6-blind-run-metadata-v1",
            "errors": [],
            "actions_executed": 0,
            "labels_read": False,
            "unscored": True,
            "score_eligible": True,
            "raw_runtime_verdict_counts": {"error_fallback": 0},
        },
        "measurements": measurements,
    }


class GeneralizationScoreTests(unittest.TestCase):
    def test_perfect_supplemental_result_passes_without_claiming_formal_r2(self) -> None:
        report = score.evaluate(SPEC, CORPUS, result_for(CORPUS))
        self.assertEqual(report["status"], "PASS")
        self.assertIs(report["supplemental_only"], True)
        self.assertEqual(report["formal_r2_effect"], "NOT_APPLICABLE")
        self.assertEqual(len(report["metrics"]["per_repo"]), 4)

    def test_missing_measurement_is_not_proven(self) -> None:
        result = result_for(CORPUS)
        result["measurements"].pop()
        report = score.evaluate(SPEC, CORPUS, result)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(any("coverage differs" in blocker for blocker in report["blockers"]))

    def test_error_fallback_is_not_proven(self) -> None:
        result = result_for(CORPUS)
        result["run_metadata"]["raw_runtime_verdict_counts"]["error_fallback"] = 1
        report = score.evaluate(SPEC, CORPUS, result)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertIn("result contains error-fallback measurements", report["blockers"])

    def test_unratified_spec_is_not_proven(self) -> None:
        spec = copy.deepcopy(SPEC)
        spec["ratification"]["status"] = "proposed"
        report = score.evaluate(spec, CORPUS, result_for(CORPUS))
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertIn("G6 metric spec is not ratified", report["blockers"])

    def test_wrong_act_fails_threshold(self) -> None:
        result = result_for(CORPUS)
        negative = next(task for task in CORPUS["tasks"] if not task["localizable"])
        row = next(
            row for row in result["measurements"] if row["task_id"] == negative["task_id"]
        )
        row["verdict"] = "act"
        report = score.evaluate(SPEC, CORPUS, result)
        self.assertEqual(report["status"], "FAIL")
        self.assertIs(report["checks"]["wrong_ground_action_rate"], False)

    def test_corpus_stratum_drift_is_not_proven(self) -> None:
        corpus = copy.deepcopy(CORPUS)
        corpus["tasks"].pop()
        result = result_for(corpus)
        report = score.evaluate(SPEC, corpus, result)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(any("expected 120" in blocker for blocker in report["blockers"]))


if __name__ == "__main__":
    unittest.main()
