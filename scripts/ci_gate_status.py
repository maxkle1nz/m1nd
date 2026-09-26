#!/usr/bin/env python3
"""Fail-closed required-check aggregator for CI draft/final lanes.

The Test check is branch-protected. Draft feedback is deliberately NOT a
release-quality receipt: a draft can pass its short lane, but Test cannot pass
until the full three-platform Rust suite has run on this exact head.
"""

from __future__ import annotations

import json
import os
import sys
from typing import Any

ALWAYS_REQUIRED = frozenset(
    {
        "ci-lane",
        "ui-gates",
        "host-pack-gates",
        "agent-cache-real-gates",
        "python-gates",
        "security-gates",
        "contract-gates",
    }
)
RUST_JOBS = frozenset({"rust-gates", "draft-rust"})


def evaluate(needs: dict[str, Any]) -> tuple[int, str]:
    """Return (exit code, human explanation) for the GitHub `needs` object."""
    expected = ALWAYS_REQUIRED | RUST_JOBS
    missing = sorted(expected - needs.keys())
    if missing:
        return 1, f"FAIL: missing required jobs: {', '.join(missing)}"

    lane_job = needs["ci-lane"]
    lane = (lane_job.get("outputs") or {}).get("lane") if isinstance(lane_job, dict) else None
    if lane not in ("draft", "full"):
        return 1, "FAIL: CI lane missing or unrecognized (no implicit pass)"

    results = {
        name: data.get("result") if isinstance(data, dict) else None
        for name, data in needs.items()
    }
    required = ALWAYS_REQUIRED | {"draft-rust" if lane == "draft" else "rust-gates"}
    skipped = {"rust-gates" if lane == "draft" else "draft-rust"}
    bad = {name: results[name] for name in sorted(required) if results[name] != "success"}
    unexpected = {name: results[name] for name in skipped if results[name] != "skipped"}
    # New dependencies must not hide failures if this script is extended without
    # its policy tests. Unknown successes are harmless; unknown skipped jobs fail.
    extra = {
        name: result
        for name, result in results.items()
        if name not in expected and result != "success"
    }
    if bad or unexpected or extra:
        return 1, "FAIL: gate result mismatch: " + json.dumps(
            {"required": bad, "other_lane": unexpected, "extra": extra}, sort_keys=True
        )

    if lane == "draft":
        return 1, (
            "Draft feedback passed, but Test cannot certify a merge: convert PR "
            "to ready for review to run the full Rust matrix on this head."
        )
    return 0, "PASS: full three-OS Rust matrix and every cumulative gate succeeded"


def main() -> int:
    try:
        needs = json.loads(os.environ["NEEDS_JSON"])
        if not isinstance(needs, dict):
            raise ValueError("needs is not an object")
        code, message = evaluate(needs)
    except (KeyError, ValueError, TypeError) as error:
        code, message = 1, f"FAIL: missing or malformed CI inputs ({error})"
    print(message)
    return code


if __name__ == "__main__":
    sys.exit(main())
