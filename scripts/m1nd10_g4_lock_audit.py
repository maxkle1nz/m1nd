#!/usr/bin/env python3
"""Mechanical R6 audit for SessionState lock boundaries.

The audit is intentionally lexical and conservative. It proves the actor owns
SessionState through BrainSessionCell::checkout (which releases the storage
mutex), refuses the historical Arc<Mutex<SessionState>> type, and inspects each
remaining legacy session-lock scope for operations that may block on durable
I/O, network, subprocesses, or graph analysis.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
SOURCE_ROOT = REPO_ROOT / "m1nd-mcp" / "src"

TYPE_REFUSALS = (
    re.compile(r"Mutex\s*<\s*(?:crate::session::)?SessionState\s*>"),
    re.compile(r"checkpoint_locked"),
)

SESSION_LOCK = re.compile(
    r"(?:\b(?:session|brain|target|victim)\s*\.\s*lock\s*\(\s*\)"
    r"|\b(?:state|app|app_state)\s*\.\s*session\s*\.\s*lock\s*\(\s*\))"
)

FORBIDDEN_IN_LOCK_SCOPE = (
    "dispatch_tool(",
    "handle_mcp_method(",
    ".persist(",
    "std::fs::",
    "fs::",
    "File::",
    "OpenOptions::",
    "TcpListener",
    "TcpStream",
    "reqwest::",
    "Command::",
    "spawn_blocking",
    ".graph.read()",
    ".graph.write()",
    "QueryOrchestrator::",
    "collect_checkpoint_files(",
    "create_checkpoint(",
    "load_current(",
)

ACTOR_METHODS = ("read_snapshot", "execute", "commit", "checkpoint_current")


def production_prefix(path: Path) -> str:
    text = path.read_text(encoding="utf-8")
    marker = "\n#[cfg(test)]\nmod tests"
    if marker in text:
        text = text.split(marker, 1)[0]
    return text


def brace_depths(lines: list[str]) -> list[int]:
    """Return lexical brace depth at the start of each line.

    Rust strings/comments can contain braces, so this is not a parser. The
    audited lock scopes use ordinary blocks; any ambiguous broadening makes the
    check conservative and can only fail, never silently pass a forbidden op.
    """

    depth = 0
    result: list[int] = []
    for line in lines:
        result.append(depth)
        depth += line.count("{") - line.count("}")
    return result


def lock_scope(lines: list[str], depths: list[int], index: int) -> tuple[int, int]:
    line = lines[index]
    declaration = re.search(
        r"\blet\s+(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=.*\.lock\s*\(\s*\)\s*;\s*$",
        line,
    )
    if declaration is None:
        # Chained field reads/writes own a temporary guard only through the
        # terminating statement.
        end = index
        while end + 1 < len(lines) and ";" not in lines[end]:
            end += 1
        return index, end

    guard = declaration.group(1)
    start_depth = depths[index]
    for cursor in range(index + 1, len(lines)):
        if re.search(rf"\bdrop\s*\(\s*{re.escape(guard)}\s*\)", lines[cursor]):
            return index, cursor
        if depths[cursor] < start_depth:
            return index, cursor - 1
    return index, len(lines) - 1


def method_body(text: str, method: str) -> str | None:
    match = re.search(rf"\n\s*fn\s+{re.escape(method)}(?:\s*<[^{{]+>)?\s*\(", text)
    if match is None:
        return None
    brace = text.find("{", match.end())
    if brace < 0:
        return None
    depth = 0
    for cursor in range(brace, len(text)):
        char = text[cursor]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return text[brace : cursor + 1]
    return None


def main() -> int:
    failures: list[dict[str, object]] = []
    audited_locks: list[dict[str, object]] = []
    production_files = sorted(
        path
        for path in SOURCE_ROOT.glob("*.rs")
        if not path.name.endswith("_tests.rs")
    )

    for path in production_files:
        text = production_prefix(path)
        relative = str(path.relative_to(REPO_ROOT))
        for refusal in TYPE_REFUSALS:
            for match in refusal.finditer(text):
                failures.append(
                    {
                        "file": relative,
                        "line": text.count("\n", 0, match.start()) + 1,
                        "reason": f"forbidden legacy pattern: {match.group(0)}",
                    }
                )

        lines = text.splitlines()
        depths = brace_depths(lines)
        consumed: set[tuple[int, int]] = set()
        for index, line in enumerate(lines):
            if SESSION_LOCK.search(line) is None:
                continue
            start, end = lock_scope(lines, depths, index)
            if (start, end) in consumed:
                continue
            consumed.add((start, end))
            scope = "\n".join(lines[start : end + 1])
            forbidden = [token for token in FORBIDDEN_IN_LOCK_SCOPE if token in scope]
            entry = {
                "file": relative,
                "line_start": start + 1,
                "line_end": end + 1,
                "forbidden_tokens": forbidden,
            }
            audited_locks.append(entry)
            if forbidden:
                failures.append(
                    {
                        **entry,
                        "reason": "blocking/analysis operation inside SessionState lock scope",
                    }
                )

    actor_path = SOURCE_ROOT / "brain_runtime.rs"
    actor_text = production_prefix(actor_path)
    for method in ACTOR_METHODS:
        body = method_body(actor_text, method)
        if body is None:
            failures.append(
                {
                    "file": str(actor_path.relative_to(REPO_ROOT)),
                    "line": None,
                    "reason": f"actor method missing from audit: {method}",
                }
            )
        elif "session.checkout()" not in body:
            failures.append(
                {
                    "file": str(actor_path.relative_to(REPO_ROOT)),
                    "line": actor_text.count("\n", 0, actor_text.find(body)) + 1,
                    "reason": f"actor method does not check out SessionState: {method}",
                }
            )

    checkout_required = (
        "let state = guard.take()",
        "drop(guard);",
        "self.cell.replace(state)",
    )
    for token in checkout_required:
        if token not in actor_text:
            failures.append(
                {
                    "file": str(actor_path.relative_to(REPO_ROOT)),
                    "line": None,
                    "reason": f"BrainSessionCell checkout/restore invariant missing: {token}",
                }
            )

    result = {
        "schema": "m1nd10-g4-r6-lock-audit-v1",
        "status": "PASS" if not failures else "FAIL",
        "production_files_scanned": len(production_files),
        "session_lock_scopes_audited": len(audited_locks),
        "actor_checkout_methods": list(ACTOR_METHODS),
        "audited_locks": audited_locks,
        "failures": failures,
        "limitations": [
            "Lexical audit is conservative and complements, but does not replace, runtime concurrency tests.",
            "Non-SessionState mutexes are outside this gate.",
        ],
    }
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
