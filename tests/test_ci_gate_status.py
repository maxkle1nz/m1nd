"""The required Test check may pass only after the full final CI lane."""

import importlib.util
from pathlib import Path
import re
import unittest

MODULE = Path(__file__).resolve().parents[1] / "scripts" / "ci_gate_status.py"
spec = importlib.util.spec_from_file_location("ci_gate_status", MODULE)
assert spec is not None and spec.loader is not None
status = importlib.util.module_from_spec(spec)
spec.loader.exec_module(status)


class CiGateStatusTest(unittest.TestCase):
    def test_draft_steps_and_rust_cache_share_target_dir(self):
        workflow = (MODULE.parents[1] / ".github/workflows/ci.yml").read_text()
        draft = workflow.split("  draft-rust:\n", 1)[1].split("  rust-gates:\n", 1)[0]
        self.assertIn("    env:\n      CARGO_TARGET_DIR: target\n", draft)
        self.assertIn("Swatinem/rust-cache@", draft)
        self.assertIn("shell: bash\n        run: bash scripts/lightning_check.sh", draft)
        script = (MODULE.parents[1] / "scripts/lightning_check.sh").read_text()
        self.assertIn('export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-', script)

    def test_every_gates_job_is_in_required_aggregate(self):
        """A new *-gates job cannot silently bypass the protected Test check."""
        workflow = (MODULE.parents[1] / ".github/workflows/ci.yml").read_text()
        jobs = list(re.finditer(r"(?m)^  ([a-z0-9-]+):\s*$", workflow))
        names = {m.group(1) for m in jobs}
        required = {name for name in names if name.endswith("-gates")}
        self.assertTrue(required)
        status_job = next((m for m in jobs if m.group(1) == "test-status"), None)
        if status_job is None:
            raise AssertionError("Protected Test job is missing")
        start = status_job.end()
        end = next((m.start() for m in jobs if m.start() > status_job.start()), len(workflow))
        block = workflow[start:end]
        needs = re.search(r"(?m)^    needs:\s*\[([^\]]+)\]\s*$", block)
        if needs is None:
            raise AssertionError("Protected Test needs list is missing")
        linked = {entry.strip() for entry in needs.group(1).split(",")}
        self.assertFalse(required - linked, f"Gate jobs omitted from Test: {required - linked}")

    def jobs(self, lane="full"):
        names = (
            "ui-gates",
            "host-pack-gates",
            "python-gates",
            "security-gates",
            "contract-gates",
        )
        jobs: dict[str, dict[str, object]] = {
            name: {"result": "success"} for name in names
        }
        jobs["ci-lane"] = {"result": "success", "outputs": {"lane": lane}}
        jobs["draft-rust"] = {"result": "skipped" if lane == "full" else "success"}
        jobs["rust-gates"] = {"result": "success" if lane == "full" else "skipped"}
        return jobs

    def test_ready_pr_merge_queue_or_main_with_all_gates_passes(self):
        self.assertEqual(status.evaluate(self.jobs())[0], 0)

    def test_draft_cannot_satisfy_required_test_check(self):
        code, message = status.evaluate(self.jobs("draft"))
        self.assertEqual(code, 1)
        self.assertIn("draft", message.lower())

    def test_final_suite_must_have_run(self):
        jobs = self.jobs()
        jobs["rust-gates"]["result"] = "skipped"
        self.assertEqual(status.evaluate(jobs)[0], 1)

    def test_failed_draft_feedback_cannot_pass(self):
        jobs = self.jobs("draft")
        jobs["draft-rust"]["result"] = "failure"
        self.assertEqual(status.evaluate(jobs)[0], 1)

    def test_no_skipped_cumulative_gate(self):
        jobs = self.jobs()
        jobs["security-gates"]["result"] = "skipped"
        self.assertEqual(status.evaluate(jobs)[0], 1)

    def test_missing_lane_fails_closed(self):
        jobs = self.jobs()
        del jobs["ci-lane"]["outputs"]
        self.assertEqual(status.evaluate(jobs)[0], 1)

    def test_unknown_lane_fails_closed(self):
        self.assertEqual(status.evaluate(self.jobs("surprise"))[0], 1)

    def test_missing_job_fails_closed(self):
        jobs = self.jobs()
        del jobs["host-pack-gates"]
        self.assertEqual(status.evaluate(jobs)[0], 1)

    def test_extra_job_with_failure_fails_closed(self):
        jobs = self.jobs()
        jobs["future-required-job"] = {"result": "failure"}
        self.assertEqual(status.evaluate(jobs)[0], 1)


if __name__ == "__main__":
    unittest.main()
