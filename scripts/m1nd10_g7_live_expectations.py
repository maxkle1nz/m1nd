#!/usr/bin/env python3
"""Compute the three digests the G7 LIVE orchestrator demands up front.

The orchestrator refuses unless the caller already knows the exact binary, UI
bundle and Chromium bundle it is about to accept -- that fail-closed order is
the point, but the digests themselves are framed tree identities nobody can
produce by hand.  This helper computes them with the orchestrator's own
functions (one hashing law, never a second implementation) and prints the
ready-to-run command.  It starts no owner, contacts no service, and writes
nothing.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import m1nd10_g7_live_orchestrator as g7  # noqa: E402


def chromium_bundle_root(ui_root: Path) -> Path:
    """Resolve the installed Chromium bundle exactly as the gate later will."""

    browsers = json.loads(
        (ui_root / "node_modules" / "playwright-core" / "browsers.json").read_bytes()
    )["browsers"]
    revision = next(row for row in browsers if row.get("name") == "chromium")[
        "revision"
    ]
    query = (
        "const {chromium}=require('playwright');"
        "process.stdout.write(chromium.executablePath());"
    )
    executable = Path(
        subprocess.run(
            ["node", "-e", query],
            cwd=ui_root,
            check=True,
            stdout=subprocess.PIPE,
            text=True,
        ).stdout.strip()
    ).resolve(strict=True)
    root_name = f"chromium-{revision}"
    root = next(
        (parent for parent in executable.parents if parent.name == root_name), None
    )
    if root is None:
        raise SystemExit(
            f"the installed Chromium is outside its locked revision directory {root_name}"
        )
    return root


def main() -> int:
    argument_parser = argparse.ArgumentParser(description=__doc__)
    argument_parser.add_argument("--source-root", type=Path, required=True)
    argument_parser.add_argument("--binary", type=Path)
    # The gate demands an absolute receipt path OUTSIDE the source root, so the
    # proof cannot dirty its own subject. Default to a sibling of the worktree.
    argument_parser.add_argument("--output", type=Path)
    args = argument_parser.parse_args()

    source_root = args.source_root.resolve(strict=True)
    output = (
        args.output.resolve()
        if args.output is not None
        else source_root.parent / "m1nd10-g7-live-receipt.json"
    )
    ui_root = source_root / "m1nd-ui"
    ui_sha256, ui_file_count = g7.ui_tree_identity(ui_root / "dist")
    browser_root = chromium_bundle_root(ui_root)
    browser_sha256, browser_files, _ = g7.framed_tree_identity(
        browser_root, g7.BROWSER_TREE_DOMAIN, label="Playwright Chromium bundle"
    )

    report = {
        "source_root": str(source_root),
        "source_clean": not subprocess.run(
            ["git", "status", "--porcelain"],
            cwd=source_root,
            check=True,
            stdout=subprocess.PIPE,
            text=True,
        ).stdout.strip(),
        "ui_bundle_sha256": ui_sha256,
        "ui_bundle_file_count": ui_file_count,
        "browser_bundle_root": str(browser_root),
        "browser_bundle_sha256": browser_sha256,
        "browser_bundle_file_count": browser_files,
        "receipt_output": str(output),
    }
    command = [
        "python3",
        "scripts/m1nd10_g7_live_orchestrator.py",
        f"--source-root {source_root}",
        f"--expected-ui-bundle-sha256 {ui_sha256}",
        f"--expected-browser-bundle-sha256 {browser_sha256}",
        f"--output {output}",
    ]
    if args.binary is not None:
        binary = args.binary.resolve(strict=True)
        report["binary"] = str(binary)
        report["binary_sha256"] = g7.sha256_file(binary)
        command[2:2] = [
            f"--binary {binary}",
            f"--expected-binary-sha256 {report['binary_sha256']}",
        ]
    report["command"] = " \\\n  ".join(command)

    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
