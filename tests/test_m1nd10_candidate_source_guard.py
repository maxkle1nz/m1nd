from __future__ import annotations

import importlib.util
import pathlib
import subprocess
import sys
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "m1nd10_candidate_source_guard.py"
SPEC = importlib.util.spec_from_file_location(
    "m1nd10_candidate_source_guard", MODULE_PATH
)
GUARD = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = GUARD
SPEC.loader.exec_module(GUARD)


class CandidateSourceGuardTests(unittest.TestCase):
    def test_public_benchmark_contract_is_allowed(self) -> None:
        paths = [
            "docs/benchmarks/m1nd10-g6-held-out-v2/public/queries.json",
            "docs/benchmarks/m1nd10-g6-held-out-v2/manifest/digests.json",
            "docs/benchmarks/m1nd10-g6-held-out-v2/schema/public.schema.json",
            "scripts/benchmark/m1nd10_g6_retrieval.py",
            "scripts/benchmark/m1nd10_g6_blind_runner.py",
        ]
        self.assertEqual([], GUARD.violations(paths))

    def test_labels_outcomes_and_label_builders_are_refused(self) -> None:
        paths = [
            "docs/benchmarks/round/operator-only/corpus.json",
            "docs/benchmarks/round/runner-results/CURRENT.json",
            "scripts/benchmark/m1nd10_g6_corpus.py",
            "tests/test_m1nd10_g6_generalization_v2_corpus.py",
        ]
        self.assertEqual(
            {
                "operator_label_source",
                "operator_private_artifact",
            },
            {row["reason"] for row in GUARD.violations(paths)},
        )

    def test_caches_local_runner_config_and_private_keys_are_refused(self) -> None:
        paths = [
            "node_modules/pkg/index.js",
            ".l00p/logs/agent.jsonl",
            "pkg/__pycache__/module.pyc",
            "docs/wiki-build/index.html",
            "m1nd-ui/tsconfig.tsbuildinfo",
            ".DS_Store",
            ".l00p-run.log",
            "runners.toml",
            "nested/runnerd.secret",
            "keys/release.pem",
        ]
        self.assertEqual(10, len(GUARD.violations(paths)))

    def test_gitlinks_symlinks_and_oversized_blobs_are_refused(self) -> None:
        entries = [
            {
                "path": "vendor/repo",
                "mode": "160000",
                "object_type": "commit",
                "size": None,
            },
            {
                "path": "linked-secret",
                "mode": "120000",
                "object_type": "blob",
                "size": 12,
            },
            {
                "path": "large.bin",
                "mode": "100644",
                "object_type": "blob",
                "size": GUARD.MAX_BLOB_BYTES + 1,
            },
        ]
        self.assertEqual(
            {
                "non_regular_git_entry",
                "oversized_blob",
            },
            {row["reason"] for row in GUARD.metadata_violations(entries)},
        )

    def test_exact_commit_scan_ignores_untracked_worktree_but_refuses_tracked_private(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = pathlib.Path(temporary)
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(repo),
                    "config",
                    "user.email",
                    "guard@example.invalid",
                ],
                check=True,
            )
            subprocess.run(
                ["git", "-C", str(repo), "config", "user.name", "Candidate Guard"],
                check=True,
            )
            public = repo / "public.txt"
            public.write_text("public\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(repo), "add", "public.txt"], check=True)
            subprocess.run(
                ["git", "-C", str(repo), "commit", "-qm", "public"], check=True
            )

            private = repo / "docs" / "round" / "operator-only" / "labels.json"
            private.parent.mkdir(parents=True)
            private.write_text("{}\n", encoding="utf-8")
            report = GUARD.inspect_candidate(repo, "HEAD")
            self.assertEqual("PASS", report["status"])
            projection = GUARD.inspect_worktree_projection(repo)
            self.assertEqual("FAIL", projection["status"])

            (repo / ".gitignore").write_text(
                "docs/**/operator-only/\n", encoding="utf-8"
            )
            projection = GUARD.inspect_worktree_projection(repo)
            self.assertEqual("PASS", projection["status"])

            subprocess.run(
                ["git", "-C", str(repo), "add", "-f", str(private)], check=True
            )
            subprocess.run(
                ["git", "-C", str(repo), "commit", "-qm", "private"], check=True
            )
            report = GUARD.inspect_candidate(repo, "HEAD")
            self.assertEqual("FAIL", report["status"])
            self.assertEqual(1, report["violation_count"])
            self.assertEqual(
                "operator_private_artifact",
                report["violations"][0]["reason"],
            )


if __name__ == "__main__":
    unittest.main()
