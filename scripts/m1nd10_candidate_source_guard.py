#!/usr/bin/env python3
"""Refuse private or generated files in an immutable M1ND source candidate."""

from __future__ import annotations

import argparse
import json
import stat
import subprocess
import sys
from pathlib import Path, PurePosixPath
from typing import Iterable


SCHEMA = "m1nd10-candidate-source-boundary-v1"

# These builders and tests contain or deterministically reconstruct held-out
# labels. Keeping only their generated public projections in the candidate is
# what makes the runner's `labels_exposed=false` contract enforceable.
OPERATOR_SOURCE_PATHS = frozenset(
    {
        "scripts/benchmark/m1nd10_g6_corpus.py",
        "scripts/benchmark/m1nd10_g6_held_out_v2_corpus.py",
        "scripts/benchmark/m1nd10_g6_generalization_v2_corpus.py",
        "tests/test_m1nd10_g6_corpus.py",
        "tests/test_m1nd10_g6_held_out_v2_corpus.py",
        "tests/test_m1nd10_g6_generalization_v2_corpus.py",
    }
)

PRIVATE_COMPONENTS = frozenset({"operator-only", "runner-results"})
CACHE_COMPONENTS = frozenset(
    {
        "node_modules",
        "__pycache__",
        ".cache",
        ".l00p",
        ".mypy_cache",
        ".pytest_cache",
        ".ruff_cache",
        "wiki-build",
    }
)
SECRET_BASENAMES = frozenset({".env", "runnerd.secret", "runners.toml"})
PRIVATE_KEY_SUFFIXES = frozenset({".key", ".p12", ".pem", ".pfx"})
GENERATED_BASENAMES = frozenset({".DS_Store"})
GENERATED_SUFFIXES = frozenset({".log", ".pyc", ".pyo", ".tsbuildinfo"})
MAX_BLOB_BYTES = 8 * 1024 * 1024


class SourceBoundaryError(RuntimeError):
    pass


def violation_for(path_text: str) -> str | None:
    """Return the first closed-set source-boundary violation for one Git path."""

    if not path_text or "\x00" in path_text or "\\" in path_text:
        return "non_canonical_path"
    path = PurePosixPath(path_text)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        return "non_canonical_path"
    if path_text in OPERATOR_SOURCE_PATHS:
        return "operator_label_source"
    if PRIVATE_COMPONENTS.intersection(path.parts):
        return "operator_private_artifact"
    if CACHE_COMPONENTS.intersection(path.parts):
        return "generated_cache"
    if path.name in GENERATED_BASENAMES or path.suffix.lower() in GENERATED_SUFFIXES:
        return "generated_cache"
    if path.name in SECRET_BASENAMES:
        return "local_secret_or_runner_config"
    if path.suffix.lower() in PRIVATE_KEY_SUFFIXES:
        return "private_key_material"
    return None


def violations(paths: Iterable[str]) -> list[dict[str, str]]:
    observed: dict[str, str] = {}
    for path in paths:
        reason = violation_for(path)
        if reason is not None:
            observed[path] = reason
    return [{"path": path, "reason": observed[path]} for path in sorted(observed)]


def run_git(repo: Path, arguments: list[str]) -> bytes:
    completed = subprocess.run(
        ["git", "-C", str(repo), *arguments],
        check=False,
        capture_output=True,
    )
    if completed.returncode != 0:
        detail = completed.stderr.decode("utf-8", errors="replace").strip()
        raise SourceBoundaryError(detail or f"git {' '.join(arguments)} failed")
    return completed.stdout


def resolve_commit(repo: Path, revision: str) -> str:
    raw = run_git(repo, ["rev-parse", "--verify", f"{revision}^{{commit}}"])
    try:
        commit = raw.decode("ascii").strip()
    except UnicodeDecodeError as error:
        raise SourceBoundaryError("Git returned a non-ASCII commit identity") from error
    if len(commit) != 40 or any(
        character not in "0123456789abcdef" for character in commit
    ):
        raise SourceBoundaryError("Git returned a non-canonical SHA-1 commit identity")
    return commit


def candidate_paths(repo: Path, commit: str) -> list[str]:
    return [entry["path"] for entry in candidate_entries(repo, commit)]


def candidate_entries(repo: Path, commit: str) -> list[dict[str, object]]:
    raw = run_git(repo, ["ls-tree", "-r", "-z", "-l", "--full-tree", commit])
    records = raw.split(b"\x00")
    if records and records[-1] == b"":
        records.pop()
    entries: list[dict[str, object]] = []
    for record in records:
        try:
            metadata, raw_path = record.split(b"\t", 1)
            mode, object_type, object_id, raw_size = metadata.split()
            path = raw_path.decode("utf-8")
            size = None if raw_size == b"-" else int(raw_size)
        except (ValueError, UnicodeDecodeError) as error:
            raise SourceBoundaryError("Git returned a malformed tree entry") from error
        entries.append(
            {
                "path": path,
                "mode": mode.decode("ascii"),
                "object_type": object_type.decode("ascii"),
                "object_id": object_id.decode("ascii"),
                "size": size,
            }
        )
    return entries


def decode_nul_paths(raw: bytes) -> list[str]:
    fields = raw.split(b"\x00")
    if fields and fields[-1] == b"":
        fields.pop()
    paths: list[str] = []
    for field in fields:
        try:
            paths.append(field.decode("utf-8"))
        except UnicodeDecodeError as error:
            raise SourceBoundaryError(
                "candidate contains a path that is not valid UTF-8"
            ) from error
    return paths


def worktree_projection_paths(repo: Path) -> list[str]:
    """Model the path set produced by `git add -A` without changing the index."""

    visible = set(
        decode_nul_paths(
            run_git(
                repo,
                ["ls-files", "-z", "--cached", "--others", "--exclude-standard"],
            )
        )
    )
    deleted = set(decode_nul_paths(run_git(repo, ["ls-files", "-z", "--deleted"])))
    return sorted(visible - deleted)


def metadata_violations(entries: Iterable[dict[str, object]]) -> list[dict[str, str]]:
    rejected: dict[str, str] = {}
    for entry in entries:
        path = str(entry.get("path", ""))
        mode = entry.get("mode")
        object_type = entry.get("object_type")
        size = entry.get("size")
        if object_type != "blob" or mode not in {"100644", "100755"}:
            rejected[path] = "non_regular_git_entry"
        elif not isinstance(size, int) or size < 0:
            rejected[path] = "invalid_blob_size"
        elif size > MAX_BLOB_BYTES:
            rejected[path] = "oversized_blob"
    return [{"path": path, "reason": rejected[path]} for path in sorted(rejected)]


def worktree_metadata_violations(
    repo: Path, paths: Iterable[str]
) -> list[dict[str, str]]:
    rejected: dict[str, str] = {}
    for path in paths:
        try:
            metadata = (repo / path).lstat()
        except OSError:
            rejected[path] = "unreadable_worktree_entry"
            continue
        if not stat.S_ISREG(metadata.st_mode):
            rejected[path] = "non_regular_worktree_entry"
        elif metadata.st_size > MAX_BLOB_BYTES:
            rejected[path] = "oversized_blob"
    return [{"path": path, "reason": rejected[path]} for path in sorted(rejected)]


def merged_violations(*groups: Iterable[dict[str, str]]) -> list[dict[str, str]]:
    rejected: dict[str, str] = {}
    for group in groups:
        for row in group:
            rejected.setdefault(row["path"], row["reason"])
    return [{"path": path, "reason": rejected[path]} for path in sorted(rejected)]


def inspect_candidate(repo: Path, revision: str) -> dict[str, object]:
    commit = resolve_commit(repo, revision)
    entries = candidate_entries(repo, commit)
    paths = [str(entry["path"]) for entry in entries]
    rejected = merged_violations(violations(paths), metadata_violations(entries))
    return {
        "schema": SCHEMA,
        "status": "PASS" if not rejected else "FAIL",
        "proof_state": "PROVEN" if not rejected else "REFUSED",
        "proof_scope": "exact_commit_path_boundary",
        "candidate_kind": "immutable_commit",
        "commit": commit,
        "max_blob_bytes": MAX_BLOB_BYTES,
        "tracked_path_count": len(paths),
        "violation_count": len(rejected),
        "violations": rejected,
    }


def inspect_worktree_projection(repo: Path) -> dict[str, object]:
    base_commit = resolve_commit(repo, "HEAD")
    paths = worktree_projection_paths(repo)
    rejected = merged_violations(
        violations(paths), worktree_metadata_violations(repo, paths)
    )
    return {
        "schema": SCHEMA,
        "status": "PASS" if not rejected else "FAIL",
        "proof_state": "PROVEN" if not rejected else "REFUSED",
        "proof_scope": "uncommitted_worktree_path_projection",
        "candidate_kind": "worktree_projection",
        "base_commit": base_commit,
        "max_blob_bytes": MAX_BLOB_BYTES,
        "tracked_path_count": len(paths),
        "violation_count": len(rejected),
        "violations": rejected,
    }


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description=__doc__)
    value.add_argument("--repo", type=Path, default=Path.cwd())
    mode = value.add_mutually_exclusive_group()
    mode.add_argument("--revision")
    mode.add_argument("--worktree-projection", action="store_true")
    return value


def main() -> int:
    args = parser().parse_args()
    try:
        repo = args.repo.resolve()
        if args.worktree_projection:
            report = inspect_worktree_projection(repo)
        else:
            report = inspect_candidate(repo, args.revision or "HEAD")
    except SourceBoundaryError as error:
        print(
            f"candidate source guard could not prove the boundary: {error}",
            file=sys.stderr,
        )
        return 2
    print(json.dumps(report, sort_keys=True, indent=2))
    return 0 if report["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
