"""Adversarial tests for the Git-diff CI classifier."""

import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest


MODULE = Path(__file__).resolve().parents[1] / "scripts" / "ci_scope.py"
spec = importlib.util.spec_from_file_location("ci_scope", MODULE)
assert spec is not None and spec.loader is not None
ci_scope = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ci_scope)


def git(repo, *args):
    return subprocess.check_output(["git", "-C", str(repo), *args], text=True).strip()


class CiScopeTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.repo = Path(self.temp.name) / "repo"
        self.repo.mkdir()
        git(self.repo, "init", "--quiet")
        git(self.repo, "config", "user.email", "ci@example.test")
        git(self.repo, "config", "user.name", "CI test")

    def tearDown(self):
        self.temp.cleanup()

    def commit(self, files):
        for name, body in files.items():
            path = self.repo / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(body)
        git(self.repo, "add", "-A")
        git(self.repo, "commit", "--quiet", "-m", "fixture")
        return git(self.repo, "rev-parse", "HEAD")

    def classify_from(self, base):
        head = git(self.repo, "rev-parse", "HEAD")
        event = self.repo / "event.json"
        event.write_text(json.dumps({"pull_request": {"base": {"sha": base}, "head": {"sha": head}}}))
        return ci_scope.classify(self.repo, "pull_request", event, head)

    def test_rename_classifies_both_old_and_new_paths(self):
        base = self.commit({"m1nd-ui/src/move-me": "same"})
        old = self.repo / "m1nd-ui/src/move-me"
        new = self.repo / "m1nd-core/src/move-me"
        new.parent.mkdir(parents=True, exist_ok=True)
        old.rename(new)
        git(self.repo, "add", "-A")
        git(self.repo, "commit", "--quiet", "-m", "rename")
        selected = self.classify_from(base)
        self.assertEqual(selected["mode"], "scoped")
        self.assertTrue(selected["ui"])
        self.assertTrue(selected["rust"])
        self.assertIn("m1nd-core", selected["crates"])

    def test_deletion_uses_the_deleted_path(self):
        base = self.commit({"m1nd-ui/src/deleted.ts": "export {};"})
        (self.repo / "m1nd-ui/src/deleted.ts").unlink()
        git(self.repo, "add", "-A")
        git(self.repo, "commit", "--quiet", "-m", "delete")
        selected = self.classify_from(base)
        self.assertEqual(selected["mode"], "scoped")
        self.assertTrue(selected["ui"])

    def test_unknown_global_empty_and_malformed_all_fall_back_to_full(self):
        candidate = "a" * 40
        self.assertEqual(ci_scope.classify_paths(candidate, ["unowned/file"])["mode"], "full")
        self.assertEqual(ci_scope.classify_paths(candidate, ["Cargo.lock"])["mode"], "full")
        self.assertEqual(ci_scope.classify_paths(candidate, [])["mode"], "full")
        self.assertIsNone(ci_scope.parse_name_status(b"R100\0only-one-path\0"))

    def test_event_candidate_or_head_mismatch_falls_back_to_actual_head(self):
        base = self.commit({"docs/one.md": "one"})
        self.commit({"docs/two.md": "two"})
        head = git(self.repo, "rev-parse", "HEAD")
        event = self.repo / "event.json"
        event.write_text(json.dumps({"pull_request": {"base": {"sha": base}, "head": {"sha": "b" * 40}}}))
        selected = ci_scope.classify(self.repo, "pull_request", event, head)
        self.assertEqual(selected["mode"], "full")
        self.assertEqual(selected["candidate"], head)

    def test_schema_rejects_extra_or_non_boolean_fields(self):
        selected = ci_scope.full("a" * 40)
        self.assertTrue(ci_scope.valid_scope(selected))
        selected["bonus"] = "not allowed"
        self.assertFalse(ci_scope.valid_scope(selected))
        selected = ci_scope.full("a" * 40)
        selected["ui"] = 1
        self.assertFalse(ci_scope.valid_scope(selected))

    def test_ui_dependency_audit_is_not_selected_by_a_plain_ui_source_edit(self):
        base = self.commit({"m1nd-ui/src/view.ts": "export const view = 1;"})
        self.commit({"m1nd-ui/src/view.ts": "export const view = 2;"})
        head = git(self.repo, "rev-parse", "HEAD")
        event = self.repo / "event.json"
        event.write_text(json.dumps({"pull_request": {"base": {"sha": base}, "head": {"sha": head}}}))
        self.assertFalse(ci_scope.dependency_change(self.repo, "pull_request", event, head, "ui"))
        self.assertFalse(ci_scope.dependency_change(self.repo, "pull_request", event, head, "cargo"))


if __name__ == "__main__":
    unittest.main()
