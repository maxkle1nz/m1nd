#!/usr/bin/env python3
"""Classify one CI candidate from its GitHub event and complete Git diff.

The only machine-readable scope contract is ``m1nd-ci-scope-v1`` below.  This
script deliberately chooses ``full`` whenever it cannot prove that a diff is a
known, bounded change.  It never trusts branch names: pull requests are read
from the event's base/head SHAs and compared to the checked-out candidate.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from typing import Iterable


SCHEMA = "m1nd-ci-scope-v1"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
CRATES = (
    "m1nd-control",
    "m1nd-core",
    "m1nd-ingest",
    "m1nd-mcp",
    "m1nd-openclaw",
    "m1nd-runnerd",
)
HEAVY = ("retrobuilder", "transplant")
GLOBAL_PREFIXES = (
    ".github/workflows/",
    ".config/",
)
GLOBAL_PATHS = frozenset(
    {
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "scripts/ci_scope.py",
        "scripts/ci_gate_status.py",
        "scripts/lightning_check.sh",
        "scripts/rust_pr_check.py",
        "scripts/m1nd10_candidate_source_guard.py",
        ".gitleaks.toml",
        "SECURITY.md",
    }
)
DOC_NAMES = frozenset({"README.md", "AGENTS.md", "CONTRIBUTING.md"})


def scope(candidate: str, mode: str, *, rust: bool, ui: bool, host: bool, cache: bool,
          python: bool, crates: Iterable[str] = (), heavy: Iterable[str] = ()) -> dict[str, object]:
    """Build the complete, canonical v1 object (and no policy side-channel)."""
    return {
        "schema": SCHEMA,
        "candidate": candidate,
        "mode": mode,
        "rust": rust,
        "ui": ui,
        "host": host,
        "cache": cache,
        "python": python,
        "crates": sorted(set(crates)),
        "heavy": sorted(set(heavy)),
    }


def full(candidate: str) -> dict[str, object]:
    return scope(
        candidate,
        "full",
        rust=True,
        ui=True,
        host=True,
        cache=True,
        python=True,
        crates=CRATES,
        heavy=HEAVY,
    )


def valid_scope(value: object) -> bool:
    """Validate the closed v1 schema used by the classifier and aggregator."""
    if not isinstance(value, dict):
        return False
    expected = {
        "schema", "candidate", "mode", "rust", "ui", "host", "cache", "python", "crates", "heavy"
    }
    if set(value) != expected or value.get("schema") != SCHEMA:
        return False
    if not isinstance(value.get("candidate"), str) or not SHA_RE.fullmatch(value["candidate"]):
        return False
    if value.get("mode") not in {"full", "scoped"}:
        return False
    if any(type(value.get(key)) is not bool for key in ("rust", "ui", "host", "cache", "python")):
        return False
    crates = value.get("crates")
    heavy = value.get("heavy")
    if not isinstance(crates, list) or not isinstance(heavy, list):
        return False
    if crates != sorted(set(crates)) or heavy != sorted(set(heavy)):
        return False
    if any(not isinstance(crate, str) or crate not in CRATES for crate in crates):
        return False
    if any(not isinstance(family, str) or family not in HEAVY for family in heavy):
        return False
    if value["mode"] == "full":
        return (
            all(value[key] for key in ("rust", "ui", "host", "cache", "python"))
            and crates == list(CRATES)
            and heavy == list(HEAVY)
        )
    return (bool(crates) == value["rust"]) and (not heavy or value["rust"])


def git(repo: Path, *args: str) -> bytes:
    return subprocess.check_output(["git", "-C", str(repo), *args], stderr=subprocess.DEVNULL)


def head_sha(repo: Path) -> str | None:
    try:
        candidate = git(repo, "rev-parse", "HEAD").decode().strip().lower()
    except (OSError, subprocess.CalledProcessError):
        return None
    return candidate if SHA_RE.fullmatch(candidate) else None


def parse_name_status(payload: bytes) -> list[tuple[str, tuple[str, ...]]] | None:
    """Parse ``git diff --name-status -z -M`` including both rename endpoints."""
    fields = payload.split(b"\0")
    if not fields or fields[-1] != b"":
        return None
    fields.pop()
    records: list[tuple[str, tuple[str, ...]]] = []
    cursor = 0
    while cursor < len(fields):
        try:
            status = fields[cursor].decode("ascii")
        except UnicodeDecodeError:
            return None
        cursor += 1
        if status in {"A", "M", "D", "T"}:
            width = 1
        elif re.fullmatch(r"[RC][0-9]{1,3}", status):
            width = 2
        else:
            return None
        if cursor + width > len(fields):
            return None
        raw_paths = fields[cursor:cursor + width]
        cursor += width
        try:
            paths = tuple(path.decode("utf-8") for path in raw_paths)
        except UnicodeDecodeError:
            return None
        if any(not path or path.startswith("/") or "\0" in path for path in paths):
            return None
        records.append((status, paths))
    return records


def crate_for(path: str) -> str | None:
    for crate in CRATES:
        prefix = f"{crate}/"
        if path.startswith(prefix + "src/") or path.startswith(prefix + "tests/"):
            return crate
    return None


def is_global(path: str) -> bool:
    if path in GLOBAL_PATHS or path.endswith("/Cargo.toml") or path.endswith("/build.rs"):
        return True
    return path.startswith(GLOBAL_PREFIXES)


def classify_paths(candidate: str, paths: Iterable[str]) -> dict[str, object]:
    """Classify known paths. Empty or any unowned path deliberately becomes full."""
    path_list = list(paths)
    if not path_list or any(is_global(path) for path in path_list):
        return full(candidate)

    rust = ui = host = cache = python = False
    crates: set[str] = set()
    heavy: set[str] = set()
    for path in path_list:
        crate = crate_for(path)
        if crate:
            rust = cache = True
            crates.add(crate)
            lowered = path.lower()
            if "retrobuilder" in lowered:
                heavy.add("retrobuilder")
            if "transplant" in lowered:
                heavy.add("transplant")
            continue
        if path.startswith("m1nd-ui/"):
            ui = True
            continue
        if path.startswith("npm/") or path in {"package.json", "package-lock.json"}:
            host = cache = True
            continue
        if path.startswith("tests/test_") and path.endswith(".py"):
            python = True
            continue
        if path == "scripts/m1nd-dev" or path.startswith("scripts/devkit/"):
            if path.startswith("scripts/devkit/ui-dev"):
                ui = True
            else:
                python = ui = cache = True
            continue
        if path in DOC_NAMES or path.startswith("docs/"):
            continue
        return full(candidate)

    if not any((rust, ui, host, cache, python)):
        # A docs-only PR keeps the invariant/security gates but runs no selected
        # area gate. This is still a scoped result, never an implicit full pass.
        return scope(candidate, "scoped", rust=False, ui=False, host=False, cache=False, python=False)

    # Static dependency closure: core and ingest test each other; their
    # dependents are MCP, runnerd and openclaw. Control feeds the same outer
    # layer through MCP. This closure is deliberately explicit so a metadata
    # failure cannot silently shrink CI coverage.
    if rust:
        if {"m1nd-core", "m1nd-ingest"} & crates:
            crates.update({"m1nd-core", "m1nd-ingest", "m1nd-mcp", "m1nd-runnerd", "m1nd-openclaw"})
        if "m1nd-control" in crates:
            crates.update({"m1nd-mcp", "m1nd-runnerd", "m1nd-openclaw"})
        if "m1nd-mcp" in crates:
            crates.update({"m1nd-runnerd", "m1nd-openclaw"})

    return scope(candidate, "scoped", rust=rust, ui=ui, host=host, cache=cache,
                 python=python, crates=crates, heavy=heavy)


def event_shas(path: Path) -> tuple[str, str] | None:
    try:
        event = json.loads(path.read_text(encoding="utf-8"))
        base = event["pull_request"]["base"]["sha"].lower()
        head = event["pull_request"]["head"]["sha"].lower()
    except (OSError, TypeError, KeyError, ValueError, AttributeError):
        return None
    if not isinstance(base, str) or not isinstance(head, str):
        return None
    if not SHA_RE.fullmatch(base) or not SHA_RE.fullmatch(head):
        return None
    return base, head


def classify(repo: Path, event_name: str, event_path: Path, requested_candidate: str | None) -> dict[str, object]:
    actual = head_sha(repo)
    if actual is None:
        # No valid candidate can be represented safely. Let the caller fail; a
        # guessed SHA would let the aggregate certify the wrong commit.
        raise RuntimeError("checked-out candidate is unavailable or malformed")
    if event_name != "pull_request":
        return full(actual)
    shas = event_shas(event_path)
    requested = requested_candidate.lower() if isinstance(requested_candidate, str) else ""
    if shas is None or not SHA_RE.fullmatch(requested) or requested != actual:
        return full(actual)
    base, event_head = shas
    if event_head != actual:
        return full(actual)
    try:
        merge_base = git(repo, "merge-base", base, event_head).decode().strip().lower()
        if not SHA_RE.fullmatch(merge_base):
            return full(actual)
        payload = git(repo, "diff", "--name-status", "-z", "-M", merge_base, event_head)
    except (OSError, subprocess.CalledProcessError):
        return full(actual)
    records = parse_name_status(payload)
    if records is None:
        return full(actual)
    return classify_paths(actual, (path for _, paths in records for path in paths))


def dependency_change(repo: Path, event_name: str, event_path: Path, requested_candidate: str | None,
                      domain: str) -> bool:
    """Whether a PR changed dependency manifests; failures conservatively return true."""
    selected = classify(repo, event_name, event_path, requested_candidate)
    if selected["mode"] == "full":
        return True
    if event_name != "pull_request":
        return True
    shas = event_shas(event_path)
    if shas is None:
        return True
    try:
        merge_base = git(repo, "merge-base", shas[0], shas[1]).decode().strip()
        payload = git(repo, "diff", "--name-status", "-z", "-M", merge_base, shas[1])
    except (OSError, subprocess.CalledProcessError):
        return True
    records = parse_name_status(payload)
    if records is None:
        return True
    paths = {path for _, record_paths in records for path in record_paths}
    cargo = any(path.endswith("/Cargo.toml") or path in {"Cargo.toml", "Cargo.lock"} for path in paths)
    ui = bool({"m1nd-ui/package.json", "m1nd-ui/package-lock.json"} & paths)
    if domain == "cargo":
        return cargo
    if domain == "ui":
        return ui
    return cargo or ui or bool({"package.json", "package-lock.json"} & paths)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--event-name", required=True)
    parser.add_argument("--event-path", type=Path, required=True)
    parser.add_argument("--candidate", default=os.environ.get("GITHUB_SHA"))
    parser.add_argument("--github-output", type=Path)
    parser.add_argument("--dependency-domain", choices=("any", "cargo", "ui"))
    args = parser.parse_args()
    try:
        if args.dependency_domain:
            print("true" if dependency_change(
                args.repo, args.event_name, args.event_path, args.candidate, args.dependency_domain
            ) else "false")
            return 0
        selected = classify(args.repo, args.event_name, args.event_path, args.candidate)
        if not valid_scope(selected):
            raise RuntimeError("classifier produced an invalid scope")
    except (RuntimeError, OSError, ValueError) as error:
        print(f"CI scope classification failed closed: {error}", file=sys.stderr)
        return 1
    encoded = json.dumps(selected, sort_keys=True, separators=(",", ":"))
    print(encoded)
    if args.github_output:
        with args.github_output.open("a", encoding="utf-8") as output:
            output.write(f"scope={encoded}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
