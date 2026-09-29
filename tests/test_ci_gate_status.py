"""The protected Test aggregate has no implicit success path."""

import importlib.util
import json
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
MODULE = ROOT / "scripts" / "ci_gate_status.py"
spec = importlib.util.spec_from_file_location("ci_gate_status", MODULE)
assert spec is not None and spec.loader is not None
status = importlib.util.module_from_spec(spec)
spec.loader.exec_module(status)


def scope(mode="scoped", *, rust=False, ui=False, host=False, cache=False, python=False):
    return {
        "schema": "m1nd-ci-scope-v1",
        "candidate": "a" * 40,
        "mode": mode,
        "rust": rust,
        "ui": ui,
        "host": host,
        "cache": cache,
        "python": python,
        "crates": (
            list(status.valid_scope.__globals__["CRATES"])
            if mode == "full"
            else ["m1nd-mcp"] if rust else []
        ),
        "heavy": [] if mode == "scoped" else list(status.valid_scope.__globals__["HEAVY"]),
    }


class CiGateStatusTest(unittest.TestCase):
    def needs_for(self, selected):
        results = {name: {"result": "skipped"} for name in status.EXPECTED}
        for name in status.ALWAYS:
            results[name] = {"result": "success"}
        for flag, job in status.AREA_JOBS.items():
            if selected[flag]:
                results[job] = {"result": "success"}
        if selected["mode"] == "full":
            results["rust-full"] = {"result": "success"}
        elif selected["rust"]:
            results["rust-scoped"] = {"result": "success"}
        return results

    def test_scoped_selected_jobs_pass_and_unselected_jobs_skip(self):
        selected = scope(rust=True, ui=True)
        self.assertEqual(status.evaluate(self.needs_for(selected), json.dumps(selected))[0], 0)

    def test_docs_scope_can_pass_with_only_the_always_gates(self):
        selected = scope()
        self.assertEqual(status.evaluate(self.needs_for(selected), json.dumps(selected))[0], 0)

    def test_full_requires_the_full_rust_matrix(self):
        selected = scope("full", rust=True, ui=True, host=True, cache=True, python=True)
        jobs = self.needs_for(selected)
        jobs["rust-full"] = {"result": "skipped"}
        self.assertEqual(status.evaluate(jobs, json.dumps(selected))[0], 1)

    def test_selected_failure_and_unselected_success_fail(self):
        selected = scope(ui=True)
        jobs = self.needs_for(selected)
        jobs["ui-gates"] = {"result": "failure"}
        self.assertEqual(status.evaluate(jobs, json.dumps(selected))[0], 1)
        jobs = self.needs_for(selected)
        jobs["host-pack-gates"] = {"result": "success"}
        self.assertEqual(status.evaluate(jobs, json.dumps(selected))[0], 1)

    def test_cancelled_missing_malformed_or_extra_job_fail(self):
        selected = scope(cache=True)
        jobs = self.needs_for(selected)
        jobs["agent-cache-real-gates"] = {"result": "cancelled"}
        self.assertEqual(status.evaluate(jobs, json.dumps(selected))[0], 1)
        jobs = self.needs_for(selected)
        del jobs["security-gates"]
        self.assertEqual(status.evaluate(jobs, json.dumps(selected))[0], 1)
        jobs = self.needs_for(selected)
        jobs["future-gate"] = {"result": "success"}
        self.assertEqual(status.evaluate(jobs, json.dumps(selected))[0], 1)
        self.assertEqual(status.evaluate(self.needs_for(selected), "{}")[0], 1)

    def test_pass_message_names_scope_and_candidate(self):
        selected = scope(ui=True)
        code, message = status.evaluate(self.needs_for(selected), json.dumps(selected))
        self.assertEqual(code, 0)
        self.assertIn("scope=scoped", message)
        self.assertIn("candidate=" + "a" * 40, message)


if __name__ == "__main__":
    unittest.main()
