import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "m1nd10_release_artifact_smoke",
    ROOT / "scripts" / "m1nd10_release_artifact_smoke.py",
)
assert SPEC and SPEC.loader
smoke = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(smoke)


class ReleaseArtifactSmokeUnitTests(unittest.TestCase):
    def test_version_binds_version_and_commit(self):
        smoke.validate_version("m1nd-mcp 1.4.0 (abcdef012)", "1.4.0", "abcdef012345")

    def test_version_refuses_wrong_version_or_commit(self):
        with self.assertRaises(smoke.SmokeError):
            smoke.validate_version("m1nd-mcp 1.3.0 (abcdef0)", "1.4.0", "abcdef012345")
        with self.assertRaises(smoke.SmokeError):
            smoke.validate_version("m1nd-mcp 1.4.0 (0123456)", "1.4.0", "abcdef012345")

    def test_free_port_is_loopback_bindable(self):
        self.assertGreater(smoke.free_loopback_port(), 0)

    def test_ui_manifest_binds_embedded_available_bytes(self):
        digest = "a" * 64
        manifest = {
            "manifest": {
                "ui": {"bundle_sha256": f"sha256:{digest}", "mode": "embedded"},
                "authorities": {
                    "ui_bundle": {
                        "digest": f"sha256:{digest}",
                        "freshness": "FRESH",
                        "status": "AVAILABLE",
                    }
                },
            }
        }
        self.assertEqual(smoke.validate_ui_manifest(manifest, digest)["sha256"], digest)

    def test_ui_manifest_refuses_placeholder_or_different_tree(self):
        digest = "a" * 64
        manifest = {
            "manifest": {
                "ui": {"bundle_sha256": f"sha256:{digest}", "mode": "embedded"},
                "authorities": {
                    "ui_bundle": {
                        "digest": f"sha256:{digest}",
                        "freshness": "UNKNOWN",
                        "status": "DEGRADED",
                    }
                },
            }
        }
        with self.assertRaises(smoke.SmokeError):
            smoke.validate_ui_manifest(manifest, digest)
        with self.assertRaises(smoke.SmokeError):
            smoke.validate_ui_manifest(manifest, "b" * 64)


if __name__ == "__main__":
    unittest.main()
