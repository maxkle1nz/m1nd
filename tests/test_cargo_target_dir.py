"""The build-artifact separation contract of `scripts/cargo_target_dir.sh`.

Two checkouts of this repo building into ONE `CARGO_TARGET_DIR` emit artifacts
with the SAME name — cargo's metadata hash does not encode the source path. The
red half of that is a gate failing on a sibling's binary; the dangerous half is a
gate PASSING on one. The helper separates them by checkout path; these are the
properties that separation has to keep.
"""

from __future__ import annotations

import os
import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
HELPER = ROOT / "scripts" / "cargo_target_dir.sh"


def target_dir(cwd: pathlib.Path, home: pathlib.Path) -> str:
    """Run the helper from `cwd` with a hermetic HOME and no inherited git env."""
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env["HOME"] = str(home)
    completed = subprocess.run(
        ["bash", str(HELPER)],
        cwd=str(cwd),
        env=env,
        check=True,
        capture_output=True,
        text=True,
    )
    return completed.stdout.strip()


def init_repo(path: pathlib.Path) -> None:
    subprocess.run(["git", "init", "-q", str(path)], check=True)
    subprocess.run(
        ["git", "-C", str(path), "config", "user.email", "cache@example.invalid"],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(path), "config", "user.name", "Target Dir Fixture"],
        check=True,
    )


class CargoTargetDirTests(unittest.TestCase):
    def test_one_checkout_resolves_to_one_stable_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            home = pathlib.Path(temporary) / "home"
            repo = pathlib.Path(temporary) / "repo-alpha"
            repo.mkdir(parents=True)
            home.mkdir()
            init_repo(repo)
            nested = repo / "crate" / "src"
            nested.mkdir(parents=True)

            first = target_dir(repo, home)
            second = target_dir(repo, home)

            self.assertEqual(first, second, "same checkout must be deterministic")
            self.assertEqual(
                first,
                target_dir(nested, home),
                "a subdirectory belongs to its checkout, not to a directory of its own",
            )
            self.assertTrue(
                first.startswith(str(home / ".m1nd-build-cache")),
                f"must stay inside the auto-deletable build cache, got {first}",
            )

    def test_two_checkouts_never_share_a_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            home = pathlib.Path(temporary) / "home"
            home.mkdir()
            alpha = pathlib.Path(temporary) / "repo-alpha"
            beta = pathlib.Path(temporary) / "repo-beta"
            for repo in (alpha, beta):
                repo.mkdir()
                init_repo(repo)

            self.assertNotEqual(target_dir(alpha, home), target_dir(beta, home))

    def test_a_linked_worktree_never_shares_its_parent_checkout_directory(self) -> None:
        """The case that bit: worktrees share an object store, not a build."""
        with tempfile.TemporaryDirectory() as temporary:
            home = pathlib.Path(temporary) / "home"
            home.mkdir()
            repo = pathlib.Path(temporary) / "repo-alpha"
            repo.mkdir()
            init_repo(repo)
            (repo / "file.txt").write_text("seed\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(repo), "add", "file.txt"], check=True)
            subprocess.run(["git", "-C", str(repo), "commit", "-qm", "seed"], check=True)

            linked = pathlib.Path(temporary) / "worktree-b"
            subprocess.run(
                ["git", "-C", str(repo), "worktree", "add", "-q", str(linked), "HEAD"],
                check=True,
            )

            # Detached HEAD, same branchless commit, same object store: only the
            # checkout path may decide, and it must.
            self.assertNotEqual(target_dir(repo, home), target_dir(linked, home))

    def test_outside_a_checkout_it_keeps_the_historical_shared_path(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            home = pathlib.Path(temporary) / "home"
            home.mkdir()
            loose = pathlib.Path(temporary) / "not-a-repo"
            loose.mkdir()

            self.assertEqual(
                target_dir(loose, home),
                str(home / ".m1nd-build-cache" / "target"),
            )


if __name__ == "__main__":
    unittest.main()
