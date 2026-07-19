import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "m1nd10_release_authority", ROOT / "scripts" / "m1nd10_release_authority.py"
)
assert SPEC and SPEC.loader
authority = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(authority)


class ReleaseAuthorityTests(unittest.TestCase):
    CRATE_VERSIONS = {
        "m1nd-core": "1.4.0",
        "m1nd-control": "0.1.0",
        "m1nd-ingest": "1.4.0",
        "m1nd-mcp": "1.4.0",
    }

    def probes(self):
        return authority.targets(
            "maxkle1nz/m1nd",
            "v1.4.0",
            "1.4.0",
            "token",
            self.CRATE_VERSIONS,
        )

    def test_every_authority_must_return_explicit_not_found(self):
        authority.require_nonexistent(self.probes(), lambda _url, _headers: 404)

    def test_existing_or_indeterminate_authority_fails_closed(self):
        with self.assertRaisesRegex(authority.ReleaseAuthorityError, "already exists"):
            authority.require_nonexistent(self.probes(), lambda _url, _headers: 200)
        with self.assertRaisesRegex(authority.ReleaseAuthorityError, "NOT_PROVEN"):
            authority.require_nonexistent(self.probes(), lambda _url, _headers: 503)

    def test_tag_and_version_must_match_exactly(self):
        with self.assertRaises(authority.ReleaseAuthorityError):
            authority.targets(
                "maxkle1nz/m1nd",
                "v1.4.1",
                "1.4.0",
                "token",
                self.CRATE_VERSIONS,
            )

    def test_crate_authority_uses_the_explicit_complete_version_set(self):
        probes = self.probes()
        crate_probes = [probe for probe in probes if probe[0].startswith("crates_version:")]
        self.assertEqual(len(crate_probes), 4)
        self.assertIn("/m1nd-control/0.1.0", crate_probes[1][1])
        parsed = authority.crate_version_map(
            [f"{name}={version}" for name, version in self.CRATE_VERSIONS.items()]
        )
        self.assertEqual(parsed, self.CRATE_VERSIONS)
        with self.assertRaises(authority.ReleaseAuthorityError):
            authority.crate_version_map(["m1nd-core=1.4.0"])


if __name__ == "__main__":
    unittest.main()
