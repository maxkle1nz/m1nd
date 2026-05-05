#!/usr/bin/env python3
"""Agent-first smoke test for the m1nd MCP stdio surface.

The smoke proves the minimum trust loop an agent needs:

initialize -> tools/list -> ingest -> seek -> help

It intentionally talks JSON-RPC over Content-Length framed stdio instead of
calling Rust internals, so it catches transport/session issues that unit tests
can miss.
"""

from __future__ import annotations

import argparse
import json
import os
import select
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any


SCHEMA = "m1nd-mcp-agent-smoke-v0"
DEFAULT_QUERY = "where MCP tool schemas and runtime tool registry are declared"
REQUIRED_TOOLS = ("ingest", "seek", "help")


class SmokeFailure(RuntimeError):
    pass


class McpStdioClient:
    def __init__(self, binary: Path, runtime_dir: Path, timeout: float, cwd: Path) -> None:
        self.binary = binary
        self.runtime_dir = runtime_dir
        self.timeout = timeout
        self.cwd = cwd
        self.proc: subprocess.Popen[bytes] | None = None
        self.next_id = 1
        self.read_buffer = bytearray()

    def __enter__(self) -> "McpStdioClient":
        self.proc = subprocess.Popen(
            [str(self.binary), "--no-gui", "--runtime-dir", str(self.runtime_dir)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=0,
            cwd=str(self.cwd),
        )
        return self

    def __exit__(self, _exc_type: object, _exc: object, _tb: object) -> None:
        if not self.proc:
            return
        try:
            if self.proc.stdin:
                self.proc.stdin.close()
        except OSError:
            pass
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        try:
            if self.proc.stderr:
                self.proc.stderr.read()
        except OSError:
            pass

    def call_rpc(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        req_id = self.next_id
        self.next_id += 1
        request = {
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
            "params": params or {},
        }
        self._write_message(request)
        response = self._read_message()
        if response.get("error"):
            raise SmokeFailure(f"{method} returned JSON-RPC error: {response['error']}")
        return response

    def call_tool(self, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        response = self.call_rpc("tools/call", {"name": name, "arguments": arguments})
        result = response.get("result") or {}
        if result.get("isError"):
            raise SmokeFailure(f"{name} returned MCP tool error: {result}")
        content = result.get("content") or []
        text = content[0].get("text") if content else None
        if not isinstance(text, str):
            raise SmokeFailure(f"{name} returned no text content")
        try:
            return json.loads(text)
        except json.JSONDecodeError as exc:
            raise SmokeFailure(f"{name} returned non-JSON content: {text[:200]}") from exc

    def _write_message(self, payload: dict[str, Any]) -> None:
        if not self.proc or not self.proc.stdin:
            raise SmokeFailure("MCP process is not running")
        raw = json.dumps(payload, separators=(",", ":")).encode("utf-8")
        self.proc.stdin.write(b"Content-Length: " + str(len(raw)).encode("ascii") + b"\r\n\r\n")
        self.proc.stdin.write(raw)
        self.proc.stdin.flush()

    def _read_message(self) -> dict[str, Any]:
        if not self.proc or not self.proc.stdout:
            raise SmokeFailure("MCP process is not running")
        header = self._read_until(b"\r\n\r\n")
        length = None
        for line in header.decode("utf-8", errors="replace").split("\r\n"):
            if line.lower().startswith("content-length:"):
                length = int(line.split(":", 1)[1].strip())
                break
        if length is None:
            raise SmokeFailure(f"response missing Content-Length header: {header!r}")
        body = self._read_exact(length)
        return json.loads(body)

    def _read_until(self, marker: bytes) -> bytes:
        if not self.proc or not self.proc.stdout:
            raise SmokeFailure("MCP process is not running")
        deadline = time.monotonic() + self.timeout
        fd = self.proc.stdout.fileno()
        while marker not in self.read_buffer:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise SmokeFailure(f"timed out waiting for response header after {self.timeout}s")
            ready, _, _ = select.select([fd], [], [], remaining)
            if not ready:
                raise SmokeFailure(f"timed out waiting for response header after {self.timeout}s")
            chunk = os.read(fd, 4096)
            if not chunk:
                raise SmokeFailure("MCP process closed stdout while reading response header")
            self.read_buffer.extend(chunk)
        index = self.read_buffer.index(marker)
        header = bytes(self.read_buffer[:index])
        del self.read_buffer[: index + len(marker)]
        return header

    def _read_exact(self, length: int) -> str:
        if not self.proc or not self.proc.stdout:
            raise SmokeFailure("MCP process is not running")
        deadline = time.monotonic() + self.timeout
        fd = self.proc.stdout.fileno()
        while len(self.read_buffer) < length:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise SmokeFailure(f"timed out waiting for response body after {self.timeout}s")
            ready, _, _ = select.select([fd], [], [], remaining)
            if not ready:
                raise SmokeFailure(f"timed out waiting for response body after {self.timeout}s")
            chunk = os.read(fd, max(4096, length - len(self.read_buffer)))
            if not chunk:
                raise SmokeFailure("MCP process closed stdout while reading response body")
            self.read_buffer.extend(chunk)
        body = bytes(self.read_buffer[:length])
        del self.read_buffer[:length]
        return body.decode("utf-8")


def default_binary(repo: Path) -> Path:
    return repo / "target" / "debug" / "m1nd-mcp"


def summarize_ingest(payload: dict[str, Any]) -> dict[str, Any]:
    return {
        "adapter": payload.get("adapter"),
        "mode": payload.get("mode"),
        "files_scanned": payload.get("files_scanned"),
        "files_parsed": payload.get("files_parsed"),
        "node_count": payload.get("node_count"),
        "edge_count": payload.get("edge_count"),
        "elapsed_ms": payload.get("elapsed_ms"),
    }


def summarize_seek(payload: dict[str, Any]) -> dict[str, Any]:
    results = payload.get("results") or []
    return {
        "proof_state": payload.get("proof_state"),
        "total_candidates_scanned": payload.get("total_candidates_scanned"),
        "results_count": len(results),
        "top_results": [
            result.get("file_path") or result.get("path") or result.get("label") or result.get("node_id")
            for result in results[:5]
        ],
        "elapsed_ms": payload.get("elapsed_ms"),
    }


def run_smoke(args: argparse.Namespace) -> dict[str, Any]:
    repo = Path(args.repo).expanduser().resolve()
    binary = Path(args.binary).expanduser().resolve() if args.binary else default_binary(repo)
    if not repo.exists():
        raise SmokeFailure(f"repo path does not exist: {repo}")
    if not binary.exists():
        raise SmokeFailure(
            f"m1nd-mcp binary does not exist: {binary}. Build it first with `cargo build -p m1nd-mcp`."
        )

    runtime_dir = Path(args.runtime_dir).expanduser().resolve() if args.runtime_dir else Path(
        tempfile.mkdtemp(prefix="m1nd-agent-smoke-")
    )
    runtime_created = not bool(args.runtime_dir)
    runtime_dir.mkdir(parents=True, exist_ok=True)

    started = time.monotonic()
    try:
        with McpStdioClient(binary=binary, runtime_dir=runtime_dir, timeout=args.timeout, cwd=repo) as client:
            initialize = client.call_rpc(
                "initialize",
                {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "m1nd-agent-smoke", "version": "0"},
                },
            )
            tools_response = client.call_rpc("tools/list")
            tools = (tools_response.get("result") or {}).get("tools") or []
            tool_names = [tool.get("name") for tool in tools]
            missing_tools = [name for name in REQUIRED_TOOLS if name not in tool_names]
            if len(tools) < args.min_tool_count:
                raise SmokeFailure(f"expected at least {args.min_tool_count} tools, got {len(tools)}")
            if missing_tools:
                raise SmokeFailure(f"missing required tools: {', '.join(missing_tools)}")

            ingest = client.call_tool(
                "ingest",
                {
                    "agent_id": args.agent_id,
                    "path": str(repo),
                    "adapter": args.adapter,
                    "mode": "replace",
                    "include_dotfiles": args.include_dotfiles,
                },
            )
            node_count = int(ingest.get("node_count") or 0)
            edge_count = int(ingest.get("edge_count") or 0)
            if node_count <= 0 or edge_count <= 0:
                raise SmokeFailure(f"ingest produced an empty graph: nodes={node_count}, edges={edge_count}")

            seek = client.call_tool(
                "seek",
                {
                    "agent_id": args.agent_id,
                    "query": args.query,
                    "top_k": args.top_k,
                    "graph_rerank": True,
                },
            )
            candidates = int(seek.get("total_candidates_scanned") or 0)
            results = seek.get("results") or []
            proof_state = seek.get("proof_state")
            if proof_state == "blocked" or candidates <= 0 or not results:
                raise SmokeFailure(
                    "seek did not see the ingested graph: "
                    f"proof_state={proof_state}, candidates={candidates}, results={len(results)}"
                )

            help_payload = client.call_tool(
                "help",
                {
                    "agent_id": args.agent_id,
                    "tool_name": "seek",
                    "mode": "tool",
                    "render": "compact",
                },
            )
            if not help_payload.get("found") or not (
                help_payload.get("guidance") or help_payload.get("formatted")
            ):
                raise SmokeFailure("help did not return usable guidance for seek")
    finally:
        if runtime_created and not args.keep_runtime_dir:
            shutil.rmtree(runtime_dir, ignore_errors=True)

    return {
        "schema": SCHEMA,
        "ok": True,
        "transport": "stdio",
        "binary": str(binary),
        "repo": str(repo),
        "runtime_dir": str(runtime_dir) if args.keep_runtime_dir or args.runtime_dir else None,
        "duration_ms": round((time.monotonic() - started) * 1000, 3),
        "initialize": (initialize.get("result") or {}).get("serverInfo"),
        "tool_count": len(tools),
        "required_tools_present": {name: name in tool_names for name in REQUIRED_TOOLS},
        "ingest": summarize_ingest(ingest),
        "seek": summarize_seek(seek),
        "help": {
            "tool": help_payload.get("tool"),
            "found": help_payload.get("found"),
            "proof_state": help_payload.get("proof_state"),
            "next_suggested_tool": help_payload.get("next_suggested_tool"),
            "has_guidance": bool(help_payload.get("guidance") or help_payload.get("formatted")),
        },
        "checks": {
            "tools_list_ok": True,
            "ingest_populated_graph": True,
            "seek_scanned_ingested_graph": True,
            "help_returned_guidance": True,
        },
    }


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run an agent-first smoke test against m1nd MCP stdio.")
    parser.add_argument("--repo", default=os.getcwd(), help="Repository path to ingest. Defaults to cwd.")
    parser.add_argument("--binary", help="Path to m1nd-mcp binary. Defaults to <repo>/target/debug/m1nd-mcp.")
    parser.add_argument("--runtime-dir", help="Runtime directory for isolated sidecar state.")
    parser.add_argument("--keep-runtime-dir", action="store_true", help="Keep the temporary runtime dir for debugging.")
    parser.add_argument("--agent-id", default="m1nd-agent-smoke", help="Agent id used for tool calls.")
    parser.add_argument("--query", default=DEFAULT_QUERY, help="Seek query to run after ingest.")
    parser.add_argument("--top-k", type=int, default=5, help="Seek result limit.")
    parser.add_argument("--adapter", default="code", help="Ingest adapter.")
    parser.add_argument("--include-dotfiles", action="store_true", help="Include allowed dotfiles during ingest.")
    parser.add_argument("--min-tool-count", type=int, default=1, help="Minimum expected tools/list count.")
    parser.add_argument("--timeout", type=float, default=20.0, help="Per-response timeout in seconds.")
    parser.add_argument("--json", action="store_true", help="Print machine-readable JSON.")
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    try:
        result = run_smoke(args)
    except SmokeFailure as exc:
        failure = {
            "schema": SCHEMA,
            "ok": False,
            "transport": "stdio",
            "error": str(exc),
        }
        print(json.dumps(failure, indent=2), file=sys.stderr if not args.json else sys.stdout)
        return 1

    if args.json:
        print(json.dumps(result, indent=2))
    else:
        print("m1nd MCP stdio smoke passed")
        print(f"- tools: {result['tool_count']}")
        print(f"- graph nodes: {result['ingest']['node_count']}")
        print(f"- graph edges: {result['ingest']['edge_count']}")
        print(f"- seek candidates: {result['seek']['total_candidates_scanned']}")
        print(f"- seek results: {result['seek']['results_count']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
