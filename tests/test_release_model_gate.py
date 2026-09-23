"""Prevent release host tests from substituting a missing embedding model."""

from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ReleaseModelGateTests(unittest.TestCase):
    def test_release_host_tests_use_a_verified_local_model(self):
        release = (ROOT / ".github/workflows/release.yml").read_text()
        ci = (ROOT / ".github/workflows/ci.yml").read_text()
        fetch_name = "      - name: Fetch and verify immutable small embedding model"
        host_name = "      - name: Host pack, update, and rollback gates"
        self.assertIn(fetch_name, release)
        release_fetch = release.split(fetch_name, 1)[1].split(host_name, 1)[0]
        release_host = release.split(host_name, 1)[1].split("      - name:", 1)[0]
        ci_fetch = ci.split(fetch_name, 1)[1].split("      - name:", 1)[0]
        hashes = re.findall(r"[0-9a-f]{64}  (?:config\.json|model\.safetensors|tokenizer\.json)", ci_fetch)
        self.assertEqual(len(hashes), 3, "CI must pin all three model files")
        for checksum in hashes:
            self.assertIn(checksum, release_fetch)
        self.assertIn("sha256sum --check --strict", release_fetch)
        self.assertIn(
            "M1ND_TEST_EMBED_MODEL: ${{ runner.temp }}/m1nd-test-embed-model",
            release_host,
        )
        self.assertIn("npm test", release_host)


if __name__ == "__main__":
    unittest.main()
