#!/usr/bin/env python3
"""Run the bounded crate proof for a validated scoped Rust CI result."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ci_scope import valid_scope  # noqa: E402


def run(command: list[str]) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run(command, check=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--scope", required=True)
    args = parser.parse_args()
    try:
        selected = json.loads(args.scope)
    except json.JSONDecodeError as error:
        parser.error(f"scope is not JSON: {error}")
    if not valid_scope(selected) or selected["mode"] != "scoped" or not selected["rust"]:
        parser.error("scope is not a selected scoped Rust result")

    excluded = []
    if "retrobuilder" not in selected["heavy"]:
        excluded.extend([
            "binary_id(m1nd-core::retrobuilder_real)",
            "binary_id(m1nd-core::retrobuilder_stress)",
        ])
    if "transplant" not in selected["heavy"]:
        excluded.append("test(transplant)")
    packages = [argument for crate in selected["crates"] for argument in ("-p", crate)]
    command = ["cargo", "nextest", "run", "--locked", *packages, "--all-targets"]
    if excluded:
        command.extend(["-E", "not (" + " | ".join(excluded) + ")"])
    run(command)
    run(["cargo", "clippy", "--locked", *packages, "--all-targets", "--", "-D", "warnings"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
