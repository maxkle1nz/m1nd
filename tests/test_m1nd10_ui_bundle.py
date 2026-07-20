import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "m1nd10_ui_bundle", ROOT / "scripts" / "m1nd10_ui_bundle.py"
)
assert SPEC and SPEC.loader
ui = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ui)


class UiBundleTests(unittest.TestCase):
    def fixture(self, root: Path) -> SimpleNamespace:
        dist = root / "dist"
        dist.mkdir()
        (dist / "index.html").write_text("<main>m1nd</main>")
        (dist / "asset.js").write_text("console.log('m1nd')")
        package_json = root / "package.json"
        package_json.write_text(json.dumps({"version": "0.1.0"}))
        package_lock = root / "package-lock.json"
        package_lock.write_text(json.dumps({"lockfileVersion": 3}))
        output = root / "UI-BUNDLE-PROVENANCE.json"
        return SimpleNamespace(
            dist=dist,
            package_json=package_json,
            package_lock=package_lock,
            commit="a" * 40,
            expected_version=None,
            node_version="v22.0.0",
            npm_version="10.0.0",
            output=output,
        )

    def test_create_and_verify_bind_tree_lock_commit_and_tools(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            digest = ui.create(args)
            verified = ui.verify(
                SimpleNamespace(
                    dist=args.dist,
                    package_json=args.package_json,
                    package_lock=args.package_lock,
                    provenance=args.output,
                    expected_commit=args.commit,
                    expected_version=None,
                    expected_sha256=digest,
                )
            )
            self.assertEqual(verified, digest)
            document = json.loads(args.output.read_text())
            self.assertEqual(document["source_commit"], args.commit)
            self.assertEqual(document["file_count"], 2)
            self.assertEqual(document["package_version"], "0.1.0")
            self.assertEqual(
                document["package_lock_sha256"], ui.sha256_file(args.package_lock)
            )

    def test_verify_refuses_changed_tree_or_lock(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            ui.create(args)
            (args.dist / "asset.js").write_text("changed")
            with self.assertRaises(ui.UiBundleError):
                ui.verify(
                    SimpleNamespace(
                        dist=args.dist,
                        package_json=args.package_json,
                        package_lock=args.package_lock,
                        provenance=args.output,
                        expected_commit=args.commit,
                        expected_version=None,
                        expected_sha256=None,
                    )
                )

    def test_create_refuses_placeholder_tree(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            (args.dist / "index.html").write_bytes(ui.PLACEHOLDER_MARKER)
            with self.assertRaises(ui.UiBundleError):
                ui.create(args)


if __name__ == "__main__":
    unittest.main()
