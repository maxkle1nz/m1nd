import importlib.util
import pathlib
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[1]
BUG_HUNT_PATH = REPO_ROOT / "scripts" / "benchmark" / "bug_hunt_round.py"


def load_bug_hunt_module():
    spec = importlib.util.spec_from_file_location("bug_hunt_round", BUG_HUNT_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class BugHuntMissionControlTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.bug_hunt = load_bug_hunt_module()

    def test_structured_mission_control_usage_completes_loop(self):
        result = {
            "mission_control_usage": {
                "mission_control_unavailable": False,
                "mission_start_called": True,
                "mission_next_count": 3,
                "mission_verify_count": 1,
                "mission_close_called": True,
                "do_not_guardrails_observed": ["call activate"],
                "verified_claims": ["claim-1"],
                "direct_proof_switches": ["step-2"],
            }
        }

        summary = self.bug_hunt.summarize_mission_control(result, [])

        self.assertTrue(summary["loop_complete"])
        self.assertFalse(summary["unavailable"])
        self.assertEqual(summary["mission_start_count"], 1)
        self.assertEqual(summary["mission_next_count"], 3)
        self.assertEqual(summary["mission_verify_count"], 1)
        self.assertEqual(summary["mission_close_count"], 1)
        self.assertEqual(summary["completed_step_count"], 4)
        self.assertEqual(summary["required_step_count"], 4)
        self.assertEqual(summary["adherence_rate"], 1.0)
        self.assertEqual(summary["do_not_guardrail_count"], 1)
        self.assertEqual(summary["verified_claim_signal_count"], 1)
        self.assertEqual(summary["direct_proof_switch_count"], 1)

    def test_default_template_does_not_create_false_unavailable_or_loop(self):
        round_payload = {"round_id": "round", "repo": "repo"}
        lane = {"lane_id": "audit-01", "instruction_mode": "m1nd-mission-control"}
        result = self.bug_hunt.result_template(round_payload, lane)

        summary = self.bug_hunt.summarize_mission_control(result, [])

        self.assertFalse(summary["unavailable"])
        self.assertFalse(summary["loop_complete"])
        self.assertEqual(summary["mission_start_count"], 0)
        self.assertEqual(summary["mission_next_count"], 0)
        self.assertEqual(summary["mission_verify_count"], 0)
        self.assertEqual(summary["mission_close_count"], 0)
        self.assertEqual(summary["completed_step_count"], 0)
        self.assertEqual(summary["required_step_count"], 4)
        self.assertEqual(summary["adherence_rate"], 0.0)


if __name__ == "__main__":
    unittest.main()
