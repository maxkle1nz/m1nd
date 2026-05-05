#!/usr/bin/env python3
"""Render a short agent-first demo transcript from the real MCP smoke harness."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any


SCHEMA = "m1nd-agent-first-demo-v0"


def run_smoke(args: argparse.Namespace, repo: Path) -> dict[str, Any]:
    smoke = repo / "scripts" / "mcp_agent_smoke.py"
    command = [
        sys.executable,
        str(smoke),
        "--repo",
        str(repo),
        "--transport",
        args.transport,
        "--query",
        args.query,
        "--top-k",
        str(args.top_k),
        "--json",
    ]
    if args.binary:
        command.extend(["--binary", args.binary])
    if args.include_dotfiles:
        command.append("--include-dotfiles")
    completed = subprocess.run(
        command,
        cwd=repo,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if completed.returncode != 0:
        detail = completed.stdout.strip() or completed.stderr.strip()
        raise RuntimeError(f"smoke failed with exit code {completed.returncode}: {detail}")
    return json.loads(completed.stdout)


def summarize(smoke: dict[str, Any]) -> dict[str, Any]:
    session = smoke.get("session_handshake") or {}
    selftest = smoke.get("trust_selftest") or session.get("trust_selftest") or {}
    ingest = smoke.get("ingest") or {}
    seek = smoke.get("seek") or {}
    negative = smoke.get("negative_recovery") or {}
    checks = smoke.get("checks") or {}
    return {
        "schema": SCHEMA,
        "ok": bool(smoke.get("ok")),
        "transport": smoke.get("transport"),
        "tool_count": smoke.get("tool_count"),
        "trust": {
            "selftest_schema": selftest.get("schema"),
            "verdict": selftest.get("verdict"),
            "next_action": selftest.get("next_action"),
            "handshake_mode": session.get("trust_mode"),
            "can_ingest": session.get("can_ingest"),
            "can_retrieve": session.get("can_retrieve"),
            "can_recover": session.get("can_recover"),
        },
        "graph": {
            "files_scanned": ingest.get("files_scanned"),
            "files_parsed": ingest.get("files_parsed"),
            "node_count": ingest.get("node_count"),
            "edge_count": ingest.get("edge_count"),
        },
        "retrieval": {
            "proof_state": seek.get("proof_state"),
            "candidates_scanned": seek.get("total_candidates_scanned"),
            "results_count": seek.get("results_count"),
            "top_results": seek.get("top_results") or [],
        },
        "recovery": {
            "negative_retrieval_tool": negative.get("next_suggested_tool"),
            "playbook_mode": negative.get("playbook_trust_mode"),
            "selftest_verdict": negative.get("selftest_verdict"),
        },
        "checks": checks,
    }


def render_markdown(summary: dict[str, Any], query: str) -> str:
    top_results = summary["retrieval"]["top_results"][:5]
    top_lines = "\n".join(f"- `{result}`" for result in top_results) or "- No results reported"
    check_lines = "\n".join(
        f"- `{name}`: {str(value).lower()}"
        for name, value in sorted((summary.get("checks") or {}).items())
    )
    return f"""# m1nd Agent-First Demo

This transcript is generated from the real `mcp_agent_smoke.py` harness. It is
not a hand-written example: it starts an MCP server, checks the host surface,
ingests the repo, runs retrieval, asks for help, calls doctor, and verifies the
recovery path for a deliberately empty query.

## Command

```bash
python3 scripts/m1nd_agent_demo.py --repo . --transport {summary["transport"]} --query {json.dumps(query)}
```

## What The Agent Learns

- Trust verdict: `{summary["trust"]["verdict"]}`
- Next action: `{summary["trust"]["next_action"]}`
- Tool surface: `{summary["tool_count"]}` tools
- Can ingest: `{str(summary["trust"]["can_ingest"]).lower()}`
- Can retrieve: `{str(summary["trust"]["can_retrieve"]).lower()}`
- Can recover: `{str(summary["trust"]["can_recover"]).lower()}`

## Graph Built

- Files scanned: `{summary["graph"]["files_scanned"]}`
- Files parsed: `{summary["graph"]["files_parsed"]}`
- Nodes: `{summary["graph"]["node_count"]}`
- Edges: `{summary["graph"]["edge_count"]}`

## Retrieval Result

- Query: `{query}`
- State: `{summary["retrieval"]["proof_state"]}`
- Candidates scanned: `{summary["retrieval"]["candidates_scanned"]}`
- Results returned: `{summary["retrieval"]["results_count"]}`

Top results:

{top_lines}

## Recovery Behavior

The demo also sends a deliberately empty query and confirms that m1nd does not
leave the agent guessing.

- Suggested tool: `{summary["recovery"]["negative_retrieval_tool"]}`
- Recovery mode: `{summary["recovery"]["playbook_mode"]}`
- Selftest verdict: `{summary["recovery"]["selftest_verdict"]}`

## Checks

{check_lines}

## Agent Reading

The important behavior is not just that retrieval returned results. The
important behavior is that the agent can tell whether the current MCP binding is
trustworthy before it acts, and when retrieval is empty it gets a structured
recovery path instead of silently falling back to blind file search.
"""


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run a concise agent-first m1nd demo.")
    parser.add_argument("--repo", default=os.getcwd(), help="Repository path to ingest. Defaults to cwd.")
    parser.add_argument("--binary", help="Path to m1nd-mcp binary. Defaults to <repo>/target/debug/m1nd-mcp.")
    parser.add_argument("--transport", choices=("stdio", "http"), default="stdio", help="Transport to demo.")
    parser.add_argument(
        "--query",
        default="where MCP tool schemas and runtime tool registry are declared",
        help="Structural query to run in the demo.",
    )
    parser.add_argument("--top-k", type=int, default=5, help="Seek result limit.")
    parser.add_argument("--include-dotfiles", action="store_true", help="Include allowed dotfiles during ingest.")
    parser.add_argument("--json", action="store_true", help="Print machine-readable JSON instead of Markdown.")
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    repo = Path(args.repo).expanduser().resolve()
    try:
        smoke = run_smoke(args, repo)
        summary = summarize(smoke)
    except Exception as exc:
        failure = {"schema": SCHEMA, "ok": False, "error": str(exc)}
        print(json.dumps(failure, indent=2) if args.json else f"m1nd agent demo failed: {exc}")
        return 1

    if args.json:
        print(json.dumps(summary, indent=2))
    else:
        print(render_markdown(summary, args.query))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
