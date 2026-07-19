from __future__ import annotations

import importlib.util
import json
import pathlib
import sys
import tempfile
import unittest


SCRIPT = pathlib.Path(__file__).parents[1] / "scripts" / "m1nd10_ground_snapshot.py"
SPEC = importlib.util.spec_from_file_location("m1nd10_ground_snapshot", SCRIPT)
assert SPEC and SPEC.loader
ground = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = ground
SPEC.loader.exec_module(ground)


class GroundSnapshotTests(unittest.TestCase):
    def _repo_and_receipt(self) -> tuple[tempfile.TemporaryDirectory[str], pathlib.Path, pathlib.Path]:
        temp = tempfile.TemporaryDirectory()
        root = pathlib.Path(temp.name)
        frozen = root / "fixture.json"
        frozen.write_text('{"value":1}\n', encoding="utf-8")
        receipt = {
            "schema": ground.SCHEMA,
            "captured_at": "2026-07-18T00:00:00Z",
            "frozen_inputs": [
                {"path": "fixture.json", "sha256": ground.sha256_file(frozen), "kind": "golden_fixture"}
            ],
            "observations": {},
            "receipt_sha256": "",
        }
        receipt["receipt_sha256"] = ground.receipt_digest(receipt)
        receipt_path = root / "receipt.json"
        receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
        return temp, root, receipt_path

    def test_valid_receipt_verifies(self) -> None:
        temp, root, receipt = self._repo_and_receipt()
        self.addCleanup(temp.cleanup)
        report = ground.verify_receipt(receipt, root)
        self.assertEqual(report.frozen_inputs_checked, 1)

    def test_frozen_input_tamper_fails(self) -> None:
        temp, root, receipt = self._repo_and_receipt()
        self.addCleanup(temp.cleanup)
        (root / "fixture.json").write_text('{"value":2}\n', encoding="utf-8")
        with self.assertRaisesRegex(ground.ReceiptError, "frozen input drift"):
            ground.verify_receipt(receipt, root)

    def test_receipt_self_tamper_fails(self) -> None:
        temp, root, receipt_path = self._repo_and_receipt()
        self.addCleanup(temp.cleanup)
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        receipt["captured_at"] = "2026-07-19T00:00:00Z"
        receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
        with self.assertRaisesRegex(ground.ReceiptError, "self-digest mismatch"):
            ground.verify_receipt(receipt_path, root)

    def test_path_escape_is_rejected_even_with_matching_digest(self) -> None:
        temp, root, receipt_path = self._repo_and_receipt()
        self.addCleanup(temp.cleanup)
        outside = root.parent / "outside-ground-snapshot-fixture"
        outside.write_text("outside", encoding="utf-8")
        self.addCleanup(lambda: outside.unlink(missing_ok=True))
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        receipt["frozen_inputs"] = [
            {"path": "../outside-ground-snapshot-fixture", "sha256": ground.sha256_file(outside)}
        ]
        receipt["receipt_sha256"] = ground.receipt_digest(receipt)
        receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
        with self.assertRaisesRegex(ground.ReceiptError, "safe repo-relative"):
            ground.verify_receipt(receipt_path, root)

    def test_canonical_digest_ignores_object_key_order_only(self) -> None:
        left = {"b": 2, "a": {"y": 1, "x": 0}, "receipt_sha256": "ignored"}
        right = {"receipt_sha256": "different", "a": {"x": 0, "y": 1}, "b": 2}
        self.assertEqual(ground.receipt_digest(left), ground.receipt_digest(right))


if __name__ == "__main__":
    unittest.main()
