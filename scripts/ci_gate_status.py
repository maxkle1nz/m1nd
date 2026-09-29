#!/usr/bin/env python3
"""Fail-closed aggregate for the strict, change-scoped CI workflow."""

from __future__ import annotations

import json
import os
from pathlib import Path
import sys
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ci_scope import valid_scope  # noqa: E402


ALWAYS = frozenset({"ci-scope", "security-gates", "contract-gates"})
AREA_JOBS = {
    "ui": "ui-gates",
    "host": "host-pack-gates",
    "cache": "agent-cache-real-gates",
    "python": "python-gates",
}
RUST_JOBS = frozenset({"rust-scoped", "rust-full"})
EXPECTED = ALWAYS | frozenset(AREA_JOBS.values()) | RUST_JOBS


def evaluate(needs: dict[str, Any], scope_json: str) -> tuple[int, str]:
    """Require every selected job to succeed and every unselected job to skip."""
    try:
        selected = json.loads(scope_json)
    except (TypeError, json.JSONDecodeError) as error:
        return 1, f"FAIL: malformed scope output ({error})"
    if not valid_scope(selected):
        return 1, "FAIL: classifier scope fails strict m1nd-ci-scope-v1 validation"
    if set(needs) != EXPECTED:
        return 1, "FAIL: CI needs differ from the closed gate set: " + json.dumps(
            {"missing": sorted(EXPECTED - set(needs)), "extra": sorted(set(needs) - EXPECTED)}, sort_keys=True
        )
    results = {
        name: data.get("result") if isinstance(data, dict) else None
        for name, data in needs.items()
    }
    required = set(ALWAYS)
    skipped: set[str] = set()
    for flag, job in AREA_JOBS.items():
        (required if selected[flag] else skipped).add(job)
    if selected["mode"] == "full":
        required.add("rust-full")
        skipped.add("rust-scoped")
    elif selected["rust"]:
        required.add("rust-scoped")
        skipped.add("rust-full")
    else:
        skipped.update(RUST_JOBS)

    failures = {name: results[name] for name in sorted(required) if results[name] != "success"}
    unexpected = {name: results[name] for name in sorted(skipped) if results[name] != "skipped"}
    if failures or unexpected:
        return 1, "FAIL: gate result mismatch: " + json.dumps(
            {"required": failures, "unselected": unexpected}, sort_keys=True
        )
    return 0, f"PASS: scope={selected['mode']} candidate={selected['candidate']}"


def main() -> int:
    try:
        needs = json.loads(os.environ["NEEDS_JSON"])
        if not isinstance(needs, dict):
            raise ValueError("needs is not an object")
        code, message = evaluate(needs, os.environ["SCOPE_JSON"])
    except (KeyError, ValueError, TypeError) as error:
        code, message = 1, f"FAIL: missing or malformed CI inputs ({error})"
    print(message)
    return code


if __name__ == "__main__":
    raise SystemExit(main())
