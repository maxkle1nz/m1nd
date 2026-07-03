#!/usr/bin/env python3
"""Agent-docs gate — fail a PR that changes agent-workflow surfaces without also
updating agent-facing documentation in the SAME PR.

Why this exists: agent behavior in this repo is defined partly by CODE (the
MCP instructions string, tool schemas, the dispatcher, the skills shipped to
hosts, the host installer). When that code changes but the docs agents read do
NOT, hosts silently teach a stale contract. This repo already paid that cost
once (installed skills taught a stale era for ~2 weeks until a manual sweep,
PR #216). This gate makes the coupling mechanical.

Anti-cry-wolf is a HARD requirement: the gate ARMS only when the diff touches a
load-bearing agent-behavior surface (SURFACE_PATHS). Unrelated internal code
(e.g. m1nd-core graph internals) does NOT arm it. When armed, it is SATISFIED by
any agent-facing doc change (DOC_PATHS) in the same PR, by a change to the
instructions string itself (self-documenting), or by the `agent-docs-exempt`
PR label (genuine no-behavioral-change refactors).

PORTABLE: to adopt in another repo, adjust SURFACE_PATHS / DOC_PATHS / the
self-documenting SELF_DOC_PATHS below. Everything else is repo-agnostic.

Usage (CI):
    python3 scripts/agent_docs_gate.py \
        --base-ref "$GITHUB_BASE_REF" \
        --event-path "$GITHUB_EVENT_PATH"

Usage (local unit test — feed synthetic inputs, no git/GitHub needed):
    GATE_FILES="m1nd-mcp/src/server.rs" GATE_LABELS="" \
        python3 scripts/agent_docs_gate.py --from-env

Exit codes: 0 = pass (not armed, satisfied, or exempt); 1 = armed & unsatisfied.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys

# --- Configuration (the only repo-specific part; edit these to port) --------

# CODE paths whose change means agent-visible behavior may have changed.
# A file "matches" a prefix if it starts with that prefix (dir prefixes end
# with "/"; file prefixes are exact-or-startswith on the full path).
SURFACE_PATHS = [
    # The MCP server: instructions string, tool schemas (all_tool_schemas_inner),
    # the tools/list + tools/call dispatch, and per-verb registration all live in
    # server.rs. Any handler/schema/verb change is agent-visible.
    "m1nd-mcp/src/server.rs",
    # Tool handlers — one per MCP verb; new/renamed public verbs land here.
    "m1nd-mcp/src/tools.rs",
    # Wire protocol: input types / schema shapes agents call against.
    "m1nd-mcp/src/protocol/",
    # help/guidance surface (help verb renders the agent-facing tool guidance).
    "m1nd-mcp/src/help_guidance.rs",
    # Universal doctrine text injected into hosts.
    "m1nd-mcp/src/universal_docs.rs",
    # Skills shipped to hosts (the exact bytes installed into codex/claude/etc.).
    "skills/",
    # npm installer host-pack logic: install-skills / mcp-config / host adapters.
    "npm/bin/",
    "npm/lib/cli.js",
    "npm/lib/agent-cli.js",
    "npm/lib/agent-schemas.js",
]

# DOC paths that satisfy the gate when the PR also touches at least one.
# (create-awareness: repo has docs/, skills/, README.md, CONTRIBUTING.md; it has
# NO root CLAUDE.md / AGENTS.md today, so those are listed but simply won't match
# until they exist — the gate never REQUIRES a file that isn't there.)
DOC_PATHS = [
    "skills/",          # skills/ is both a surface AND a doc surface: touching it satisfies.
    "docs/",            # includes docs/wiki/** and all PRDs.
    "README.md",
    "CONTRIBUTING.md",
    "CLAUDE.md",        # not present today; harmless until created.
    "AGENTS.md",        # not present today; harmless until created.
]

# The instructions-string self-documenting case (a PR whose only surface change
# is the M1ND_INSTRUCTIONS region of server.rs) is detected from the diff content
# in main() and passed to evaluate() as `instructions_only`; there is no static
# path for it because it is a region-level, not file-level, distinction.

EXEMPT_LABEL = "agent-docs-exempt"

# Marker that the M1ND_INSTRUCTIONS region changed (used for the self-doc case).
INSTRUCTIONS_MARKER = "M1ND_INSTRUCTIONS"


def _matches_any(path: str, prefixes: list[str]) -> bool:
    for p in prefixes:
        if p.endswith("/"):
            if path.startswith(p):
                return True
        else:
            if path == p or path.startswith(p + "/"):
                return True
    return False


def matched_surfaces(files: list[str]) -> list[str]:
    return [f for f in files if _matches_any(f, SURFACE_PATHS)]


def matched_docs(files: list[str]) -> list[str]:
    return [f for f in files if _matches_any(f, DOC_PATHS)]


def _git_changed_files(base_ref: str) -> list[str]:
    """Diff the PR against its merge-base with base_ref (three-dot)."""
    # Prefer the fetched remote ref when running in CI; fall back to the bare ref.
    for candidate in (f"origin/{base_ref}", base_ref):
        try:
            out = subprocess.check_output(
                ["git", "diff", "--name-only", f"{candidate}...HEAD"],
                text=True,
                stderr=subprocess.DEVNULL,
            )
            break
        except subprocess.CalledProcessError:
            continue
    else:
        print(f"agent-docs-gate: could not diff against '{base_ref}'", file=sys.stderr)
        return []
    return [line.strip() for line in out.splitlines() if line.strip()]


def _instructions_region_changed(base_ref: str) -> bool:
    """True if the diff touches lines mentioning M1ND_INSTRUCTIONS in server.rs.

    Best-effort: an instructions-only edit is self-documenting. If git isn't
    available (unit-test mode), this returns False and callers rely on
    GATE_INSTRUCTIONS_ONLY instead.
    """
    for candidate in (f"origin/{base_ref}", base_ref):
        try:
            out = subprocess.check_output(
                ["git", "diff", f"{candidate}...HEAD", "--", "m1nd-mcp/src/server.rs"],
                text=True,
                stderr=subprocess.DEVNULL,
            )
            return INSTRUCTIONS_MARKER in out
        except subprocess.CalledProcessError:
            continue
    return False


def _labels_from_event(event_path: str) -> list[str]:
    if not event_path or not os.path.exists(event_path):
        return []
    try:
        with open(event_path, encoding="utf-8") as fh:
            event = json.load(fh)
    except (OSError, json.JSONDecodeError):
        return []
    pr = event.get("pull_request") or {}
    return [lbl.get("name", "") for lbl in pr.get("labels", []) if isinstance(lbl, dict)]


FRIENDLY_MESSAGE_TEMPLATE = (
    "This PR changes agent-workflow surfaces ({matched}) but does not update any "
    "agent-facing docs. Agents read the docs, not the diff — a surface change "
    "without a doc change silently teaches a stale contract (this repo already "
    "hit that once, PR #216). Please update the agent-facing docs in THIS PR "
    "(skills/, docs/ including the wiki, README.md, CONTRIBUTING.md, or "
    "CLAUDE.md/AGENTS.md) so the change is reflected where agents will read it — "
    "or, if this is genuinely a no-agent-visible-behavior refactor, add the "
    "`agent-docs-exempt` label to skip the gate."
)


def evaluate(files: list[str], labels: list[str], instructions_only: bool) -> tuple[int, str]:
    """Core decision. Returns (exit_code, human_message)."""
    surfaces = matched_surfaces(files)
    if not surfaces:
        return 0, "agent-docs-gate: not armed (no agent-workflow surfaces touched). PASS."

    if EXEMPT_LABEL in labels:
        return 0, (
            f"agent-docs-gate: armed ({len(surfaces)} surface file(s)) but "
            f"`{EXEMPT_LABEL}` label present. PASS (exempt)."
        )

    docs = matched_docs(files)
    if docs:
        return 0, (
            "agent-docs-gate: armed and satisfied — agent-facing docs updated in the "
            f"same PR ({', '.join(docs)}). PASS."
        )

    if instructions_only:
        return 0, (
            "agent-docs-gate: armed but the only surface change is the "
            "M1ND_INSTRUCTIONS string, which is self-documenting. PASS."
        )

    return 1, FRIENDLY_MESSAGE_TEMPLATE.format(matched=", ".join(surfaces))


def _split_env(name: str) -> list[str]:
    raw = os.environ.get(name, "")
    # Accept newline- or comma-separated lists.
    parts: list[str] = []
    for chunk in raw.replace(",", "\n").splitlines():
        chunk = chunk.strip()
        if chunk:
            parts.append(chunk)
    return parts


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Agent-docs gate")
    parser.add_argument("--base-ref", default=os.environ.get("GITHUB_BASE_REF", "main"))
    parser.add_argument("--event-path", default=os.environ.get("GITHUB_EVENT_PATH", ""))
    parser.add_argument(
        "--from-env",
        action="store_true",
        help="Unit-test mode: read GATE_FILES / GATE_LABELS / GATE_INSTRUCTIONS_ONLY from env instead of git/GitHub.",
    )
    args = parser.parse_args(argv)

    if args.from_env:
        files = _split_env("GATE_FILES")
        labels = _split_env("GATE_LABELS")
        instructions_only = os.environ.get("GATE_INSTRUCTIONS_ONLY", "").strip().lower() in (
            "1",
            "true",
            "yes",
        )
    else:
        files = _git_changed_files(args.base_ref)
        labels = _labels_from_event(args.event_path)
        # Self-documenting case: only surface touched is server.rs AND the diff
        # is within the instructions region.
        surfaces = matched_surfaces(files)
        instructions_only = (
            surfaces == ["m1nd-mcp/src/server.rs"]
            and _instructions_region_changed(args.base_ref)
            and not matched_docs(files)
        )

    exit_code, message = evaluate(files, labels, instructions_only)
    stream = sys.stderr if exit_code != 0 else sys.stdout
    print(message, file=stream)
    if files:
        print(f"agent-docs-gate: changed files considered: {len(files)}", file=stream)
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
