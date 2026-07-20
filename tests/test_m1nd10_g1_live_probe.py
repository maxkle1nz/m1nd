from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "scripts" / "m1nd10_g1_live_probe.py"
SPEC = importlib.util.spec_from_file_location("m1nd10_g1_live_probe", SCRIPT)
assert SPEC and SPEC.loader
probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe)


class G1LiveProbeTests(unittest.TestCase):
    def manifest(self, observed_at: int) -> dict:
        return {
            "schema": "m1nd-organism-manifest-v1",
            "organism_id": "m1nd",
            "repo_id": "m1nd",
            "brain_id": "brain:one",
            "project_root_fingerprint": "sha256:root",
            "source": {"commit": "abc", "dirty": True, "version": "1.4.0"},
            "runtime": {
                "owner_id": "owner",
                "binary_version": "1.4.0",
                "binary_sha256": "sha256:bin",
                "started_at": 1,
            },
            "graph": {
                "generation": 1,
                "snapshot_sha256": "sha256:graph",
                "node_count": 2,
                "edge_count": 1,
            },
            "architecture": {
                "store_version": 0,
                "skeleton_digest": "",
                "ratification_state": "unavailable",
            },
            "ui": {
                "bundle_version": "0.1.0",
                "bundle_sha256": "sha256:ui",
                "mode": "embedded",
            },
            "capabilities": {"policy_version": "UNAVAILABLE", "enabled_effects": []},
            "autonomy": {"active_mode": "UNKNOWN", "issuance_frozen": True},
            "schemas": {"mission": "v1"},
            "authorities": {
                "source": {
                    "revision": "1.4.0",
                    "digest": "abc",
                    "observed_at": observed_at,
                    "freshness": "FRESH",
                    "status": "DRIFT",
                }
            },
            "release_provenance": {"release_candidate_digest": "", "signature": ""},
            "generated_at": observed_at,
            "manifest_sha256": f"digest-{observed_at}",
        }

    def test_stable_projection_ignores_only_observation_time_and_self_hash(self) -> None:
        first = self.manifest(10)
        second = self.manifest(20)
        self.assertEqual(probe.stable_projection(first), probe.stable_projection(second))
        second["runtime"]["binary_sha256"] = "sha256:changed"
        self.assertNotEqual(probe.stable_projection(first), probe.stable_projection(second))

    def test_manifest_response_requires_exact_schema_and_self_digest(self) -> None:
        manifest = self.manifest(10)
        response = {
            "schema": "m1nd-organism-manifest-response-v1",
            "manifest": manifest,
            "verification": {
                "coherence": "DRIFT",
                "computed_manifest_sha256": manifest["manifest_sha256"],
                "issues": [],
            },
        }
        self.assertIs(probe.require_manifest_response(response), manifest)
        response["verification"]["computed_manifest_sha256"] = "wrong"
        with self.assertRaisesRegex(ValueError, "self-digest"):
            probe.require_manifest_response(response)

    def test_sha256_file_uses_prefixed_lower_hex(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "binary"
            path.write_bytes(b"m1nd")
            self.assertEqual(
                probe.sha256_file(path),
                "sha256:533934b349b9a5a824014171c796b222c66326db5915f23fa193fb8842b44ee5",
            )

    def test_owner_bearer_requires_private_canonical_token_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "http-auth-token-v1"
            path.write_text("ab" * 32 + "\n", encoding="utf-8")
            path.chmod(0o600)
            self.assertEqual(probe.read_owner_bearer_token(path), "ab" * 32)

            path.write_text("AB" * 32 + "\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "canonical"):
                probe.read_owner_bearer_token(path)

            path.write_text("ab" * 32 + "\n", encoding="utf-8")
            path.chmod(0o644)
            with self.assertRaisesRegex(ValueError, "permissions"):
                probe.read_owner_bearer_token(path)


if __name__ == "__main__":
    unittest.main()
