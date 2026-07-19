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

            # A case-variant private component is also caught in the enforced
            # exact-commit mode, where .gitignore performs no filtering.
            variant = repo / "docs" / "other" / "RUNNER-RESULTS" / "current.json"
            variant.parent.mkdir(parents=True)
            variant.write_text("{}\n", encoding="utf-8")
            subprocess.run(
                ["git", "-C", str(repo), "add", "-f", str(variant)], check=True
            )
            subprocess.run(
                ["git", "-C", str(repo), "commit", "-qm", "variant"], check=True
            )
            report = GUARD.inspect_candidate(repo, "HEAD")
            self.assertEqual("FAIL", report["status"])
            self.assertEqual(2, report["violation_count"])
            self.assertEqual(
                {"operator_private_artifact"},
                {row["reason"] for row in report["violations"]},
            )

    def test_adversarial_path_policy_reasons_are_exact(self) -> None:
        cases = [
            ("docs/x/Operator-Only/labels.json", "operator_private_artifact"),
            ("RUNNER-RESULTS/current.json", "operator_private_artifact"),
            ("Node_Modules/pkg/index.js", "generated_cache"),
            (".L00P/logs/agent.jsonl", "generated_cache"),
            ("nested/.DS_STORE", "generated_cache"),
            ("Runners.TOML", "local_secret_or_runner_config"),
            (".env", "credential_file"),
            ("config/.env.local", "credential_file"),
            ("config/.ENV.LOCAL", "credential_file"),
            (".npmrc", "credential_file"),
            (".pypirc", "credential_file"),
            (".netrc", "credential_file"),
            (".git-credentials", "credential_file"),
            (".cargo/credentials.toml", "credential_file"),
            (".aws/credentials", "credential_file"),
            ("home/dev/.ssh/id_ed25519", "private_key_material"),
            ("id_rsa", "private_key_material"),
            ("deploy/deploy_ed25519", "private_key_material"),
            ("secrets/a.p8", "private_key_material"),
            ("secrets/a.der", "private_key_material"),
            ("secrets/a.jks", "private_key_material"),
            ("secrets/a.keystore", "private_key_material"),
            ("keys/release.PEM", "private_key_material"),
            ("dist/a.zip", "opaque_archive"),
            ("dist/a.tar", "opaque_archive"),
            ("dist/a.tgz", "opaque_archive"),
            ("dist/a.tbz2", "opaque_archive"),
            ("dist/a.txz", "opaque_archive"),
            ("dist/a.gz", "opaque_archive"),
            ("dist/a.bz2", "opaque_archive"),
            ("dist/a.xz", "opaque_archive"),
            ("dist/a.7z", "opaque_archive"),
            ("dist/a.rar", "opaque_archive"),
            ("dist/a.jar", "opaque_archive"),
            ("dist/a.tar.gz", "opaque_archive"),
            ("dist/Backup.ZIP", "opaque_archive"),
            ("docs/benchmarks/m1nd10-g6-held-out-v2/public/queries.json", None),
            ("scripts/benchmark/m1nd10_g6_retrieval.py", None),
            ("m1nd-core/src/lib.rs", None),
            ("README.md", None),
            ("credentials.toml", None),
            ("config/credentials.toml", None),
        ]
        for path_text, reason in cases:
            with self.subTest(path=path_text):
                self.assertEqual(reason, GUARD.violation_for(path_text))

    def test_personal_path_content_is_refused_in_worktree_and_exact_commit(
        self,
    ) -> None:
        # Offensive payloads are assembled by byte concatenation so the literal
        # path never appears contiguously in this test source; otherwise the
        # content gate would refuse the test file itself when it scans the tree.
        payloads = {
            "macos.rs": b"// see /Us" + b"ers/alice/repo/main.rs here",
            "linux.txt": b"log at /ho" + b"me/bob/project/run.txt today",
            "windows.txt": b"path C:" + b"\\Us" + b"ers\\dave\\repo noted",
            "binary.bin": b"\x00\x01\x02" + b"/Us" + b"ers/eve/x/s" + b"\x00\xff",
            "placeholder.txt": b"documented <repo-root>/docs is allowed\n",
            "clean.txt": b"nothing personal in this candidate blob\n",
        }
        offenders = {"macos.rs", "linux.txt", "windows.txt", "binary.bin"}
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
            (repo / "seed.txt").write_text("seed\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(repo), "add", "seed.txt"], check=True)
            subprocess.run(
                ["git", "-C", str(repo), "commit", "-qm", "seed"], check=True
            )
            for name, payload in payloads.items():
                (repo / name).write_bytes(payload)

            projection = GUARD.inspect_worktree_projection(repo)
            self.assertEqual("FAIL", projection["status"])
            self.assertEqual(
                offenders, {row["path"] for row in projection["violations"]}
            )
            self.assertEqual(
                {"personal_path_content"},
                {row["reason"] for row in projection["violations"]},
            )

            subprocess.run(["git", "-C", str(repo), "add", "-f", "."], check=True)
            subprocess.run(
                ["git", "-C", str(repo), "commit", "-qm", "candidate"], check=True
            )
            report = GUARD.inspect_candidate(repo, "HEAD")
            self.assertEqual("FAIL", report["status"])
            self.assertEqual(offenders, {row["path"] for row in report["violations"]})
            self.assertEqual(
                {"personal_path_content"},
                {row["reason"] for row in report["violations"]},
            )


if __name__ == "__main__":
    unittest.main()
