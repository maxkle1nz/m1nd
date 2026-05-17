import importlib.util
import json
import pathlib
import tempfile
import unittest
from types import SimpleNamespace


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

        summary = self.bug_hunt.summarize_mission_control(
            result, [], "m1nd-mission-control"
        )

        self.assertTrue(summary["applicable"])
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

        summary = self.bug_hunt.summarize_mission_control(
            result, [], "m1nd-mission-control"
        )

        self.assertTrue(summary["applicable"])
        self.assertFalse(summary["unavailable"])
        self.assertFalse(summary["loop_complete"])
        self.assertEqual(summary["mission_start_count"], 0)
        self.assertEqual(summary["mission_next_count"], 0)
        self.assertEqual(summary["mission_verify_count"], 0)
        self.assertEqual(summary["mission_close_count"], 0)
        self.assertEqual(summary["completed_step_count"], 0)
        self.assertEqual(summary["required_step_count"], 4)
        self.assertEqual(summary["adherence_rate"], 0.0)

    def test_non_mission_control_lane_does_not_emit_mission_metrics(self):
        result = {
            "notes": "mission_start mission_next mission_verify mission_close",
            "mission_control_usage": {
                "mission_control_unavailable": True,
                "mission_start_called": True,
                "mission_next_count": 1,
                "mission_verify_count": 1,
                "mission_close_called": True,
            },
        }

        summary = self.bug_hunt.summarize_mission_control(result, [], "m1nd-trained")

        self.assertFalse(summary["applicable"])
        self.assertIsNone(summary["unavailable"])
        self.assertIsNone(summary["loop_complete"])
        self.assertIsNone(summary["mission_next_count"])
        self.assertIsNone(summary["adherence_rate"])

    def test_structured_unavailable_mission_control_does_not_count_text_tokens(self):
        result = {
            "notes": "mission_start mission_next mission_verify mission_close",
            "mission_control_usage": {
                "mission_control_unavailable": True,
                "mission_start_called": True,
                "mission_next_count": 1,
                "mission_verify_count": 1,
                "mission_close_called": True,
            },
        }

        summary = self.bug_hunt.summarize_mission_control(
            result, [], "m1nd-mission-control"
        )

        self.assertTrue(summary["applicable"])
        self.assertTrue(summary["unavailable"])
        self.assertFalse(summary["loop_complete"])
        self.assertEqual(summary["completed_step_count"], 0)
        self.assertEqual(summary["adherence_rate"], 0.0)

    def test_mission_control_validity_marks_partial_lanes_not_evaluable(self):
        lanes = [
            {
                "lane_id": "audit-01",
                "instruction_mode": "m1nd-mission-control",
                "completed": True,
                "mission_control": {"loop_complete": True, "unavailable": False},
            },
            {
                "lane_id": "audit-02",
                "instruction_mode": "m1nd-mission-control",
                "completed": True,
                "mission_control": {"loop_complete": False, "unavailable": False},
            },
            {
                "lane_id": "audit-03",
                "instruction_mode": "m1nd-mission-control",
                "completed": False,
            },
            {
                "lane_id": "audit-04",
                "instruction_mode": "direct",
                "completed": True,
            },
        ]

        validity = self.bug_hunt.mission_control_validity(lanes)

        self.assertTrue(validity["present"])
        self.assertFalse(validity["all_completed_lanes_evaluable"])
        self.assertEqual(validity["lane_count"], 3)
        self.assertEqual(validity["evaluable_lane_ids"], ["audit-01"])
        self.assertEqual(validity["partial_or_unavailable_lane_ids"], ["audit-02"])
        self.assertEqual(validity["missing_result_lane_ids"], ["audit-03"])

    def test_preflight_accepts_mission_control_round_scaffold(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            out_dir = pathlib.Path(temp_dir)
            args = SimpleNamespace(
                out_dir=out_dir,
                round_id="round",
                repo="repo",
                source_repo=None,
                seeded_repo=None,
                workspace_root=None,
                source_commit=None,
                seeded_bug_count=1,
                lanes_full_spec=0,
                lanes_temponizer_full=0,
                lanes_temponizer_compact=0,
                lanes_temponizer=0,
                lanes_short_audit=0,
                lanes_mission_control=1,
                lanes_trained=0,
                lanes_basic=0,
                lanes_direct=1,
            )
            self.bug_hunt.init_round(args)

            preflight = self.bug_hunt.preflight_round(
                out_dir / "round.json",
                required_modes=["m1nd-mission-control", "direct"],
            )

            self.assertTrue(preflight["ok"])
            self.assertEqual(preflight["blockers"], [])
            self.assertEqual(preflight["arm_lane_counts"]["m1nd-mission-control"], 1)
            self.assertEqual(preflight["arm_lane_counts"]["direct"], 1)

    def test_preflight_requires_live_mission_control_tools(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            out_dir = pathlib.Path(temp_dir)
            args = SimpleNamespace(
                out_dir=out_dir,
                round_id="round",
                repo="repo",
                source_repo=None,
                seeded_repo=None,
                workspace_root=None,
                source_commit=None,
                seeded_bug_count=1,
                lanes_full_spec=0,
                lanes_temponizer_full=0,
                lanes_temponizer_compact=0,
                lanes_temponizer=0,
                lanes_short_audit=0,
                lanes_mission_control=1,
                lanes_trained=0,
                lanes_basic=0,
                lanes_direct=0,
            )
            self.bug_hunt.init_round(args)
            fake_mcp = self.write_fake_mcp(out_dir, ["trust_selftest", "seek"])

            preflight = self.bug_hunt.preflight_round(
                out_dir / "round.json",
                required_modes=["m1nd-mission-control"],
                require_live_mission_control=True,
                m1nd_binary=str(fake_mcp),
            )

            self.assertFalse(preflight["ok"])
            self.assertEqual(preflight["mission_control_surface"]["tool_count"], 2)
            self.assertTrue(
                any(
                    "live Mission Control tools unavailable" in item
                    for item in preflight["blockers"]
                )
            )

    def test_preflight_accepts_live_mission_control_tools(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            out_dir = pathlib.Path(temp_dir)
            args = SimpleNamespace(
                out_dir=out_dir,
                round_id="round",
                repo="repo",
                source_repo=None,
                seeded_repo=None,
                workspace_root=None,
                source_commit=None,
                seeded_bug_count=1,
                lanes_full_spec=0,
                lanes_temponizer_full=0,
                lanes_temponizer_compact=0,
                lanes_temponizer=0,
                lanes_short_audit=0,
                lanes_mission_control=1,
                lanes_trained=0,
                lanes_basic=0,
                lanes_direct=0,
            )
            self.bug_hunt.init_round(args)
            fake_mcp = self.write_fake_mcp(
                out_dir,
                [
                    "mission_start",
                    "mission_next",
                    "mission_verify",
                    "mission_close",
                    "trust_selftest",
                ],
            )

            preflight = self.bug_hunt.preflight_round(
                out_dir / "round.json",
                required_modes=["m1nd-mission-control"],
                require_live_mission_control=True,
                m1nd_binary=str(fake_mcp),
            )

            self.assertTrue(preflight["ok"])
            self.assertEqual(preflight["blockers"], [])
            self.assertTrue(
                preflight["mission_control_surface"]["mission_control_available"]
            )

    def test_preflight_rejects_broken_mission_control_prompt(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            out_dir = pathlib.Path(temp_dir)
            args = SimpleNamespace(
                out_dir=out_dir,
                round_id="round",
                repo="repo",
                source_repo=None,
                seeded_repo=None,
                workspace_root=None,
                source_commit=None,
                seeded_bug_count=1,
                lanes_full_spec=0,
                lanes_temponizer_full=0,
                lanes_temponizer_compact=0,
                lanes_temponizer=0,
                lanes_short_audit=0,
                lanes_mission_control=1,
                lanes_trained=0,
                lanes_basic=0,
                lanes_direct=0,
            )
            round_payload = self.bug_hunt.init_round(args)
            prompt_path = pathlib.Path(round_payload["lanes"][0]["prompt"])
            prompt_path.write_text("broken prompt", encoding="utf-8")

            preflight = self.bug_hunt.preflight_round(
                out_dir / "round.json",
                required_modes=["m1nd-mission-control"],
            )

            self.assertFalse(preflight["ok"])
            self.assertTrue(
                any(
                    "mission-control prompt missing tokens" in item
                    for item in preflight["blockers"]
                )
            )

    def write_fake_mcp(self, out_dir, tool_names):
        fake_mcp = pathlib.Path(out_dir) / "fake-m1nd-mcp.py"
        script = f"""#!/usr/bin/env python3
import json
import sys

TOOLS = {json.dumps(tool_names)}

for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    response = {{"jsonrpc": "2.0", "id": request.get("id")}}
    if method == "initialize":
        response["result"] = {{"serverInfo": {{"name": "fake-m1nd-mcp"}}}}
    elif method == "tools/list":
        response["result"] = {{"tools": [{{"name": name}} for name in TOOLS]}}
    else:
        response["error"] = {{"code": -32601, "message": "not found"}}
    print(json.dumps(response), flush=True)
"""
        fake_mcp.write_text(script, encoding="utf-8")
        fake_mcp.chmod(0o755)
        return fake_mcp


if __name__ == "__main__":
    unittest.main()
