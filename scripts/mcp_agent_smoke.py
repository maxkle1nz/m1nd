#!/usr/bin/env python3
"""Agent-first smoke test for the m1nd MCP stdio and HTTP surfaces.

The smoke proves the minimum trust loop an agent needs:

initialize -> tools/list -> session_handshake -> recovery_playbook when needed -> ingest -> seek -> help -> doctor

It intentionally talks JSON-RPC over Content-Length framed stdio instead of
calling Rust internals, so it catches transport/session issues that unit tests
can miss. If the host surface is missing a required recovery tool, the failure
is structured as degraded_host_tool_surface with a ready doctor payload. New
binaries expose the handshake as an MCP tool; older binaries use the local
harness fallback.
"""

from __future__ import annotations

import argparse
import json
import os
import select
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any


SCHEMA = "m1nd-mcp-agent-smoke-v0"
HANDSHAKE_SCHEMA = "m1nd-session-handshake-v0"
RECOVERY_PLAYBOOK_SCHEMA = "m1nd-recovery-playbook-v0"
DEFAULT_QUERY = "where MCP tool schemas and runtime tool registry are declared"
REQUIRED_TOOLS = ("ingest", "seek", "help", "recovery_playbook", "doctor")
HANDSHAKE_REQUIRED_TOOLS = ("health", "recovery_playbook", "doctor", "ingest", "seek", "help")


class SmokeFailure(RuntimeError):
    def __init__(self, message: str, details: dict[str, Any] | None = None) -> None:
        super().__init__(message)
        self.details = details or {}


def find_free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


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

    def initialize(self) -> dict[str, Any]:
        response = self.call_rpc(
            "initialize",
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "m1nd-agent-smoke", "version": "0"},
            },
        )
        return (response.get("result") or {}).get("serverInfo") or {}

    def list_tools(self) -> list[dict[str, Any]]:
        response = self.call_rpc("tools/list")
        return (response.get("result") or {}).get("tools") or []

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


class McpHttpClient:
    def __init__(self, binary: Path, runtime_dir: Path, timeout: float, cwd: Path, port: int) -> None:
        self.binary = binary
        self.runtime_dir = runtime_dir
        self.timeout = timeout
        self.cwd = cwd
        self.port = port
        self.base_url = f"http://127.0.0.1:{port}"
        self.proc: subprocess.Popen[bytes] | None = None

    def __enter__(self) -> "McpHttpClient":
        self.proc = subprocess.Popen(
            [
                str(self.binary),
                "--serve",
                "--bind",
                "127.0.0.1",
                "--port",
                str(self.port),
                "--runtime-dir",
                str(self.runtime_dir),
            ],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=0,
            cwd=str(self.cwd),
        )
        self._wait_ready()
        return self

    def __exit__(self, _exc_type: object, _exc: object, _tb: object) -> None:
        if not self.proc:
            return
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

    def _wait_ready(self) -> None:
        deadline = time.monotonic() + self.timeout
        last_error = ""
        while time.monotonic() < deadline:
            if self.proc and self.proc.poll() is not None:
                raise SmokeFailure(f"HTTP MCP process exited early with code {self.proc.returncode}")
            try:
                self.list_tools()
                return
            except SmokeFailure as exc:
                last_error = str(exc)
                time.sleep(0.1)
        raise SmokeFailure(f"HTTP MCP server did not become ready after {self.timeout}s: {last_error}")

    def initialize(self) -> dict[str, Any]:
        health = self._request("GET", "/api/health")
        return {
            "name": "m1nd-mcp",
            "status": health.get("status"),
            "domain": health.get("domain"),
            "node_count_before_ingest": health.get("node_count"),
            "edge_count_before_ingest": health.get("edge_count"),
        }

    def list_tools(self) -> list[dict[str, Any]]:
        payload = self._request("GET", "/api/tools")
        return payload.get("tools") or []

    def call_tool(self, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        payload = self._request("POST", f"/api/tools/{name}", arguments)
        result = payload.get("result")
        if not isinstance(result, dict):
            raise SmokeFailure(f"{name} returned no JSON object result over HTTP: {payload}")
        return result

    def _request(self, method: str, path: str, payload: dict[str, Any] | None = None) -> dict[str, Any]:
        body = None
        headers = {"Accept": "application/json"}
        if payload is not None:
            body = json.dumps(payload).encode("utf-8")
            headers["Content-Type"] = "application/json"
        request = urllib.request.Request(
            self.base_url + path,
            data=body,
            headers=headers,
            method=method,
        )
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                raw = response.read().decode("utf-8")
        except urllib.error.HTTPError as exc:
            detail = exc.read().decode("utf-8", errors="replace")
            raise SmokeFailure(f"HTTP {method} {path} failed: {exc.code} {detail}") from exc
        except OSError as exc:
            raise SmokeFailure(f"HTTP {method} {path} failed: {exc}") from exc
        try:
            return json.loads(raw)
        except json.JSONDecodeError as exc:
            raise SmokeFailure(f"HTTP {method} {path} returned non-JSON: {raw[:200]}") from exc


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


def build_tool_surface_report(
    tool_names: list[str],
    min_tool_count: int,
    required_tools: tuple[str, ...] = REQUIRED_TOOLS,
) -> dict[str, Any]:
    available = sorted({name for name in tool_names if isinstance(name, str) and name})
    missing_required = [name for name in required_tools if name not in available]
    below_minimum = len(available) < min_tool_count
    status = "degraded_host_tool_surface" if missing_required or below_minimum else "ok"
    report: dict[str, Any] = {
        "status": status,
        "tool_count": len(available),
        "min_tool_count": min_tool_count,
        "required_tools": list(required_tools),
        "required_tools_present": {name: name in available for name in required_tools},
        "missing_required_tools": missing_required,
        "available_tools_sample": available[:24],
        "degraded_host_tool_surface": status != "ok",
    }
    if report["degraded_host_tool_surface"]:
        report["recovery"] = {
            "suggested_tool": "recovery_playbook" if "recovery_playbook" in available else ("doctor" if "doctor" in available else None),
            "arguments": {
                "agent_id": "m1nd-agent-smoke",
                "observed_tool": "tools/list",
                "observed_proof_state": "blocked",
                "observed_tool_count": len(available),
                "available_tools": available,
                "missing_tools": missing_required,
            },
            "fallback": "if doctor is unavailable, restart or rebind the MCP host surface and use direct repo reads for final truth",
        }
    return report


def summarize_health(payload: dict[str, Any] | None) -> dict[str, Any] | None:
    if payload is None:
        return None
    contract = payload.get("tool_surface_contract") or {}
    alignment = payload.get("host_binding_alignment") or {}
    return {
        "status": payload.get("status"),
        "node_count": payload.get("node_count"),
        "edge_count": payload.get("edge_count"),
        "queries_processed": payload.get("queries_processed"),
        "active_session_count": len(payload.get("active_sessions") or []),
        "binding_fingerprint_schema": (payload.get("binding_fingerprint") or {}).get("schema"),
        "tool_surface_contract_schema": contract.get("schema"),
        "registry_tool_count": contract.get("registry_tool_count"),
        "host_binding_alignment_status": alignment.get("status"),
    }


def validate_health_surface_contract(payload: dict[str, Any]) -> None:
    contract = payload.get("tool_surface_contract") or {}
    if contract.get("schema") != "m1nd-tool-surface-contract-v0":
        raise SmokeFailure(f"health did not expose tool surface contract: {payload}")
    required_host_tools = set(contract.get("required_host_visible_tools") or [])
    if not {"health", "session_handshake", "recovery_playbook"}.issubset(required_host_tools):
        raise SmokeFailure(f"health contract does not name required host binding tools: {contract}")
    alignment = payload.get("host_binding_alignment") or {}
    if alignment.get("schema") != "m1nd-host-binding-alignment-v0":
        raise SmokeFailure(f"health did not expose host binding alignment guidance: {payload}")


def build_doctor_args_from_surface(agent_id: str, tool_surface: dict[str, Any]) -> dict[str, Any]:
    recovery = tool_surface.get("recovery") or {}
    arguments = dict(recovery.get("arguments") or {})
    arguments["agent_id"] = agent_id
    return arguments


def call_recovery_playbook(client: Any, agent_id: str, arguments: dict[str, Any]) -> dict[str, Any]:
    playbook_args = dict(arguments)
    playbook_args["agent_id"] = agent_id
    playbook = client.call_tool("recovery_playbook", playbook_args)
    if playbook.get("schema") != RECOVERY_PLAYBOOK_SCHEMA:
        raise SmokeFailure(f"recovery_playbook returned unexpected schema: {playbook.get('schema')}")
    steps = playbook.get("steps") or []
    if not isinstance(steps, list) or not steps:
        raise SmokeFailure(f"recovery_playbook returned no ordered steps: {playbook}")
    if not playbook.get("binding_fingerprint"):
        raise SmokeFailure(f"recovery_playbook returned no binding_fingerprint: {playbook}")
    return playbook


def run_session_handshake_from_observed(
    client: Any,
    args: argparse.Namespace,
    initialize: dict[str, Any],
    tools: list[dict[str, Any]],
    *,
    run_probe: bool,
) -> dict[str, Any]:
    tool_names = [tool.get("name") for tool in tools]
    available_tool_names = [name for name in tool_names if isinstance(name, str)]
    tool_surface = build_tool_surface_report(
        available_tool_names,
        args.min_tool_count,
        HANDSHAKE_REQUIRED_TOOLS,
    )
    can_ingest = "ingest" in available_tool_names
    can_retrieve = "seek" in available_tool_names
    can_recover = "recovery_playbook" in available_tool_names
    can_diagnose = "doctor" in available_tool_names
    health_payload = None
    doctor_payload = None
    recovery_playbook_payload = None
    probe_payload = None
    trust_mode = "full_trust"
    next_action = "continue with m1nd-first retrieval; use compiler/tests for runtime truth"

    if tool_surface["degraded_host_tool_surface"]:
        trust_mode = "degraded_host_tool_surface"
        next_action = (
            "treat m1nd as orientation only, refresh the MCP binding, "
            "and verify final truth with local files"
        )
        if can_recover:
            recovery_playbook_payload = call_recovery_playbook(
                client,
                args.agent_id,
                build_doctor_args_from_surface(args.agent_id, tool_surface),
            )
        if can_diagnose:
            doctor_payload = client.call_tool(
                "doctor",
                build_doctor_args_from_surface(args.agent_id, tool_surface),
            )
    elif "health" in available_tool_names:
        health_payload = client.call_tool("health", {"agent_id": args.agent_id})
        node_count = int(health_payload.get("node_count") or 0)
        edge_count = int(health_payload.get("edge_count") or 0)
        if node_count <= 0 or edge_count <= 0:
            trust_mode = "needs_ingest" if can_ingest else "orientation_only"
            next_action = "run ingest for the intended repo before trusting graph retrieval"
            if can_recover:
                recovery_playbook_payload = call_recovery_playbook(
                    client,
                    args.agent_id,
                    {
                        "observed_tool": "health",
                        "observed_proof_state": "blocked",
                        "observed_candidates": 0,
                    },
                )
            if can_diagnose:
                doctor_payload = client.call_tool(
                    "doctor",
                    {
                        "agent_id": args.agent_id,
                        "observed_tool": "health",
                        "observed_proof_state": "blocked",
                        "observed_candidates": 0,
                    },
                )
        validate_health_surface_contract(health_payload)
        if run_probe and can_retrieve:
            probe_payload = client.call_tool(
                "seek",
                {
                    "agent_id": args.agent_id,
                    "query": args.query,
                    "top_k": min(args.top_k, 3),
                    "graph_rerank": True,
                },
            )
            candidates = int(probe_payload.get("total_candidates_scanned") or 0)
            results = probe_payload.get("results") or []
            proof_state = probe_payload.get("proof_state")
            if proof_state == "blocked" or candidates <= 0 or not results:
                trust_mode = "stale_binding_suspected"
                next_action = "call recovery_playbook with the probe payload, then follow its ordered steps"
                recovery = probe_payload.get("recovery") or {}
                doctor_args = dict(recovery.get("arguments") or {})
                if not doctor_args:
                    doctor_args = {
                        "agent_id": args.agent_id,
                        "observed_tool": "seek",
                        "observed_proof_state": proof_state,
                        "observed_candidates": candidates,
                    }
                doctor_args["agent_id"] = args.agent_id
                if can_recover:
                    recovery_playbook_payload = call_recovery_playbook(
                        client,
                        args.agent_id,
                        doctor_args,
                    )
                if can_diagnose:
                    doctor_payload = client.call_tool("doctor", doctor_args)

    return {
        "schema": HANDSHAKE_SCHEMA,
        "trust_mode": trust_mode,
        "can_ingest": can_ingest,
        "can_retrieve": can_retrieve,
        "can_recover": can_recover,
        "next_action": next_action,
        "initialize": initialize,
        "tool_surface": tool_surface,
        "health": summarize_health(health_payload),
        "doctor": doctor_payload,
        "recovery_playbook": recovery_playbook_payload,
        "probe": summarize_seek(probe_payload) if probe_payload else None,
        "used_probe": bool(probe_payload),
    }


def attach_handshake_probe(
    client: Any,
    args: argparse.Namespace,
    handshake: dict[str, Any],
    available_tool_names: list[str],
) -> dict[str, Any]:
    if not args.handshake_probe or not handshake.get("can_retrieve"):
        return handshake
    if handshake.get("trust_mode") != "full_trust":
        return handshake

    probe_payload = client.call_tool(
        "seek",
        {
            "agent_id": args.agent_id,
            "query": args.query,
            "top_k": min(args.top_k, 3),
            "graph_rerank": True,
        },
    )
    handshake["probe"] = summarize_seek(probe_payload)
    handshake["used_probe"] = True

    candidates = int(probe_payload.get("total_candidates_scanned") or 0)
    results = probe_payload.get("results") or []
    proof_state = probe_payload.get("proof_state")
    if proof_state == "blocked" or candidates <= 0 or not results:
        handshake["trust_mode"] = "stale_binding_suspected"
        handshake["next_action"] = "call recovery_playbook with the probe payload, then follow its ordered steps"
        recovery = probe_payload.get("recovery") or {}
        doctor_args = dict(recovery.get("arguments") or {})
        if not doctor_args:
            doctor_args = {
                "agent_id": args.agent_id,
                "observed_tool": "seek",
                "observed_proof_state": proof_state,
                "observed_candidates": candidates,
            }
        doctor_args["agent_id"] = args.agent_id
        handshake["doctor_recovery"] = {
            "suggested_tool": "recovery_playbook" if "recovery_playbook" in available_tool_names else ("doctor" if "doctor" in available_tool_names else None),
            "arguments": doctor_args,
        }
        if "recovery_playbook" in available_tool_names:
            handshake["recovery_playbook"] = call_recovery_playbook(
                client,
                args.agent_id,
                doctor_args,
            )
        if "doctor" in available_tool_names:
            handshake["doctor"] = client.call_tool("doctor", doctor_args)

    return handshake


def run_session_handshake_from_live_surface(
    client: Any,
    args: argparse.Namespace,
    initialize: dict[str, Any],
    tools: list[dict[str, Any]],
    *,
    run_probe: bool,
) -> dict[str, Any]:
    tool_names = [tool.get("name") for tool in tools]
    available_tool_names = [name for name in tool_names if isinstance(name, str)]
    if "session_handshake" in available_tool_names:
        handshake = client.call_tool(
            "session_handshake",
            {
                "agent_id": args.agent_id,
                "observed_tool_count": len(available_tool_names),
                "available_tools": available_tool_names,
            },
        )
        handshake["initialize"] = initialize
        handshake["tool_count"] = len(available_tool_names)
        if not run_probe:
            return handshake
        return attach_handshake_probe(client, args, handshake, available_tool_names)

    return run_session_handshake_from_observed(
        client,
        args,
        initialize,
        tools,
        run_probe=run_probe,
    )


def run_agent_loop(client: Any, args: argparse.Namespace, repo: Path) -> dict[str, Any]:
    initialize = client.initialize()
    tools = client.list_tools()
    session_handshake = run_session_handshake_from_live_surface(
        client,
        args,
        initialize,
        tools,
        run_probe=False,
    )
    tool_names = [tool.get("name") for tool in tools]
    tool_surface = build_tool_surface_report(
        [name for name in tool_names if isinstance(name, str)],
        args.min_tool_count,
    )
    if tool_surface["degraded_host_tool_surface"]:
        doctor = None
        recovery_playbook = None
        tool_surface["recovery"]["arguments"]["agent_id"] = args.agent_id
        if tool_surface["recovery"]["suggested_tool"] == "recovery_playbook":
            recovery_playbook = call_recovery_playbook(
                client,
                args.agent_id,
                dict(tool_surface["recovery"]["arguments"]),
            )
        if "doctor" in [name for name in tool_names if isinstance(name, str)]:
            doctor_args = dict(tool_surface["recovery"]["arguments"])
            doctor = client.call_tool("doctor", doctor_args)
        details = {
            "surface_status": "degraded_host_tool_surface",
            "tool_surface": tool_surface,
            "session_handshake": session_handshake,
            "recovery_playbook": session_handshake.get("recovery_playbook") or recovery_playbook,
            "doctor": session_handshake.get("doctor") or doctor,
        }
        missing = tool_surface["missing_required_tools"]
        raise SmokeFailure(
            "degraded_host_tool_surface: "
            + (
                f"missing required tools: {', '.join(missing)}"
                if missing
                else f"expected at least {args.min_tool_count} tools, got {len(tools)}"
            ),
            details,
        )

    health_contract = client.call_tool("health", {"agent_id": args.agent_id})
    validate_health_surface_contract(health_contract)

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
    if not help_payload.get("found") or not (help_payload.get("guidance") or help_payload.get("formatted")):
        raise SmokeFailure("help did not return usable guidance for seek")

    doctor = client.call_tool(
        "doctor",
        {
            "agent_id": args.agent_id,
            "observed_tool": "seek",
            "observed_proof_state": proof_state,
            "observed_candidates": candidates,
        },
    )
    if doctor.get("schema") != "m1nd-doctor-v0":
        raise SmokeFailure(f"doctor returned unexpected schema: {doctor.get('schema')}")
    diagnostics = doctor.get("diagnostics") or {}
    if not diagnostics.get("graph_has_nodes"):
        raise SmokeFailure(f"doctor does not see the ingested graph: {diagnostics}")

    negative_seek = client.call_tool(
        "seek",
        {
            "agent_id": args.agent_id,
            "query": "qzxqzxqzxqzx_jjvjjvjjvjjv",
            "top_k": args.top_k,
            "graph_rerank": True,
        },
    )
    recovery = negative_seek.get("recovery") or {}
    graph_state = negative_seek.get("graph_state") or {}
    if negative_seek.get("proof_state") != "blocked":
        raise SmokeFailure(f"negative seek should be blocked, got {negative_seek.get('proof_state')}")
    if negative_seek.get("next_suggested_tool") != "recovery_playbook":
        raise SmokeFailure(f"negative seek did not suggest recovery_playbook: {negative_seek}")
    if recovery.get("suggested_tool") != "recovery_playbook":
        raise SmokeFailure(f"negative seek did not include recovery_playbook payload: {negative_seek}")
    if int(graph_state.get("node_count") or 0) <= 0:
        raise SmokeFailure(f"negative seek did not include populated graph_state: {negative_seek}")
    recovery_playbook = call_recovery_playbook(
        client,
        args.agent_id,
        dict(recovery.get("arguments") or {}),
    )
    if recovery_playbook.get("trust_mode") != "stale_binding_suspected":
        raise SmokeFailure(f"negative recovery_playbook did not flag stale binding: {recovery_playbook}")
    if not recovery_playbook.get("binding_fingerprint"):
        raise SmokeFailure(f"negative recovery_playbook omitted binding_fingerprint: {recovery_playbook}")

    return {
        "initialize": initialize,
        "tool_count": len(tools),
        "session_handshake": session_handshake,
        "tool_surface": tool_surface,
        "health_contract": summarize_health(health_contract),
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
        "doctor": {
            "schema": doctor.get("schema"),
            "status": doctor.get("status"),
            "graph_has_nodes": diagnostics.get("graph_has_nodes"),
            "stale_binding_suspected": diagnostics.get("stale_binding_suspected"),
            "warnings": doctor.get("warnings") or [],
        },
        "negative_recovery": {
            "proof_state": negative_seek.get("proof_state"),
            "next_suggested_tool": negative_seek.get("next_suggested_tool"),
            "recovery_tool": recovery.get("suggested_tool"),
            "playbook_schema": recovery_playbook.get("schema"),
            "playbook_trust_mode": recovery_playbook.get("trust_mode"),
            "playbook_next_action": recovery_playbook.get("next_action"),
            "playbook_step_count": len(recovery_playbook.get("steps") or []),
            "graph_node_count": graph_state.get("node_count"),
            "observed_tool": ((recovery.get("arguments") or {}).get("observed_tool")),
        },
        "checks": {
            "tools_list_ok": True,
            "ingest_populated_graph": True,
            "seek_scanned_ingested_graph": True,
            "help_returned_guidance": True,
            "doctor_confirmed_graph": True,
            "health_surface_contract_exposed": True,
            "negative_retrieval_suggested_recovery_playbook": True,
            "negative_recovery_playbook_validated": True,
        },
    }


def run_session_handshake(client: Any, args: argparse.Namespace) -> dict[str, Any]:
    initialize = client.initialize()
    tools = client.list_tools()
    return run_session_handshake_from_live_surface(
        client,
        args,
        initialize,
        tools,
        run_probe=args.handshake_probe,
    )


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
        if args.transport == "stdio":
            with McpStdioClient(binary=binary, runtime_dir=runtime_dir, timeout=args.timeout, cwd=repo) as client:
                result = run_session_handshake(client, args) if args.handshake_only else run_agent_loop(client, args, repo)
        elif args.transport == "http":
            port = args.port or find_free_port()
            with McpHttpClient(
                binary=binary,
                runtime_dir=runtime_dir,
                timeout=args.timeout,
                cwd=repo,
                port=port,
            ) as client:
                result = run_session_handshake(client, args) if args.handshake_only else run_agent_loop(client, args, repo)
                result["port"] = port
                result["base_url"] = client.base_url
        else:
            raise SmokeFailure(f"unsupported transport: {args.transport}")
    finally:
        if runtime_created and not args.keep_runtime_dir:
            shutil.rmtree(runtime_dir, ignore_errors=True)

    return {
        "schema": HANDSHAKE_SCHEMA if args.handshake_only else SCHEMA,
        "ok": True,
        "transport": args.transport,
        "binary": str(binary),
        "repo": str(repo),
        "runtime_dir": str(runtime_dir) if args.keep_runtime_dir or args.runtime_dir else None,
        "duration_ms": round((time.monotonic() - started) * 1000, 3),
        **result,
    }


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run an agent-first smoke test against m1nd MCP transports.")
    parser.add_argument("--repo", default=os.getcwd(), help="Repository path to ingest. Defaults to cwd.")
    parser.add_argument("--binary", help="Path to m1nd-mcp binary. Defaults to <repo>/target/debug/m1nd-mcp.")
    parser.add_argument("--transport", choices=("stdio", "http"), default="stdio", help="Transport to smoke.")
    parser.add_argument("--port", type=int, help="HTTP port to use when --transport=http. Defaults to a free port.")
    parser.add_argument("--runtime-dir", help="Runtime directory for isolated sidecar state.")
    parser.add_argument("--keep-runtime-dir", action="store_true", help="Keep the temporary runtime dir for debugging.")
    parser.add_argument("--agent-id", default="m1nd-agent-smoke", help="Agent id used for tool calls.")
    parser.add_argument("--query", default=DEFAULT_QUERY, help="Seek query to run after ingest.")
    parser.add_argument("--top-k", type=int, default=5, help="Seek result limit.")
    parser.add_argument("--adapter", default="code", help="Ingest adapter.")
    parser.add_argument("--include-dotfiles", action="store_true", help="Include allowed dotfiles during ingest.")
    parser.add_argument("--min-tool-count", type=int, default=1, help="Minimum expected tools/list count.")
    parser.add_argument("--timeout", type=float, default=20.0, help="Per-response timeout in seconds.")
    parser.add_argument("--handshake-only", action="store_true", help="Run only the lightweight session trust handshake.")
    parser.add_argument("--handshake-probe", action="store_true", help="Add one tiny seek probe to the session handshake.")
    parser.add_argument("--json", action="store_true", help="Print machine-readable JSON.")
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    try:
        result = run_smoke(args)
    except SmokeFailure as exc:
        failure = {
            "schema": HANDSHAKE_SCHEMA if args.handshake_only else SCHEMA,
            "ok": False,
            "transport": args.transport,
            "error": str(exc),
        }
        failure.update(exc.details)
        print(json.dumps(failure, indent=2), file=sys.stderr if not args.json else sys.stdout)
        return 1

    if args.json:
        print(json.dumps(result, indent=2))
    else:
        if args.handshake_only:
            print(f"m1nd MCP {result['transport']} session handshake passed")
            print(f"- trust mode: {result['trust_mode']}")
            print(f"- can ingest: {result['can_ingest']}")
            print(f"- can retrieve: {result['can_retrieve']}")
            print(f"- can recover: {result['can_recover']}")
            print(f"- next action: {result['next_action']}")
        else:
            print(f"m1nd MCP {result['transport']} smoke passed")
            print(f"- tools: {result['tool_count']}")
            print(f"- trust mode: {result['session_handshake']['trust_mode']}")
            print(f"- graph nodes: {result['ingest']['node_count']}")
            print(f"- graph edges: {result['ingest']['edge_count']}")
            print(f"- seek candidates: {result['seek']['total_candidates_scanned']}")
            print(f"- seek results: {result['seek']['results_count']}")
            print(f"- doctor: {result['doctor']['status']}")
            print(
                "- negative recovery: "
                f"{result['negative_recovery']['next_suggested_tool']} "
                f"({result['negative_recovery']['playbook_trust_mode']})"
            )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
