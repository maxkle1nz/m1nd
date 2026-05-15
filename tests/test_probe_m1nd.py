import importlib.util
import pathlib
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[1]
PROBE_PATH = REPO_ROOT / "skills" / "m1nd-operator" / "scripts" / "probe_m1nd.py"


def load_probe_module():
    spec = importlib.util.spec_from_file_location("probe_m1nd", PROBE_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class ProbeM1ndShortAuditTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.probe = load_probe_module()

    def test_trust_needs_ingest_for_empty_graph(self):
        result = {
            "isError": False,
            "payload": {
                "verdict": "needs_ingest",
                "graph_state": {"node_count": 0},
            },
        }
        self.assertTrue(self.probe.trust_needs_ingest(result))

    def test_orientation_arguments_bind_scope(self):
        arguments = self.probe.orientation_arguments(
            "search",
            agent_id="agent",
            repo="/repo",
            query="retry flow",
            top_k=3,
        )
        self.assertEqual(arguments["agent_id"], "agent")
        self.assertEqual(arguments["scope"], "/repo")
        self.assertEqual(arguments["query"], "retry flow")
        self.assertEqual(arguments["top_k"], 3)

    def test_short_audit_envelope_forces_direct_proof_handoff(self):
        envelope = self.probe.short_audit_envelope(
            agent_id="agent",
            repo="/repo",
            query="retry flow",
            orientation_tool="search",
            top_k=5,
            skip_ingest=False,
            trust_before={
                "isError": False,
                "payload": {
                    "schema": "m1nd-trust-selftest-v0",
                    "verdict": "needs_ingest",
                    "graph_state": {"node_count": 0, "edge_count": 0},
                },
            },
            ingest_result={
                "isError": False,
                "payload": {"nodes_created": 12, "edges_created": 24},
            },
            trust_after={
                "isError": False,
                "payload": {
                    "schema": "m1nd-session-handshake-v0",
                    "graph_state": {"node_count": 12, "edge_count": 24},
                },
            },
            orientation_result={
                "isError": False,
                "payload": {
                    "proof_state": "blocked",
                    "results": [],
                    "graph_state": {"node_count": 12, "edge_count": 24},
                },
            },
        )
        self.assertEqual(envelope["schema"], "m1nd-short-audit-helper-v0")
        self.assertTrue(envelope["switch_to_direct_proof"])
        self.assertEqual(envelope["recommendation"], "switch_to_direct_proof")
        self.assertEqual(envelope["m1nd_usage_mode"], "recovery_overhead")
        self.assertTrue(envelope["ingest_performed"])
        self.assertEqual(envelope["calls"][-1]["proof_state"], "blocked")
        self.assertIn("source files", envelope["fallback_reason"])


if __name__ == "__main__":
    unittest.main()
