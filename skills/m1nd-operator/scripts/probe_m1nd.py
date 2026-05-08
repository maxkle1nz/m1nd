#!/usr/bin/env python3

import argparse
import json
import os
import shlex
import subprocess
import sys
import shutil
from typing import Any


DEFAULT_BINARY = (
    os.environ.get("M1ND_MCP_BINARY")
    or os.environ.get("M1ND_MCP_BIN")
    or shutil.which("m1nd-mcp")
    or "m1nd-mcp"
)
DEFAULT_ARGS = shlex.split(os.environ.get("M1ND_MCP_ARGS", "--stdio --no-gui"))


class McpClient:
    def __init__(self, binary: str, extra_args: list[str]) -> None:
        self.proc = subprocess.Popen(
            [binary, *extra_args],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self._initialize()

    def _request(self, payload: dict[str, Any]) -> dict[str, Any]:
        assert self.proc.stdin is not None
        assert self.proc.stdout is not None
        self.proc.stdin.write(json.dumps(payload) + "\n")
        self.proc.stdin.flush()
        line = self.proc.stdout.readline()
        if not line:
            stderr = ""
            if self.proc.stderr is not None:
                stderr = self.proc.stderr.read().strip()
            raise RuntimeError(f"no response from m1nd-mcp; stderr={stderr}")
        return json.loads(line)

    def _initialize(self) -> None:
        response = self._request(
            {"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}}
        )
        if "error" in response:
            raise RuntimeError(f"initialize failed: {json.dumps(response, indent=2)}")

    def tools(self) -> dict[str, Any]:
        return self._request(
            {"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}
        )

    def call(self, tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        return self._request(
            {
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {"name": tool_name, "arguments": arguments},
            }
        )

    def close(self) -> None:
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.proc.kill()


def parse_embedded_json(text: str) -> Any:
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return text


def print_json(value: Any) -> None:
    print(json.dumps(value, indent=2, ensure_ascii=True))


def main() -> int:
    parser = argparse.ArgumentParser(description="Probe the local m1nd MCP runtime.")
    parser.add_argument("--binary", default=DEFAULT_BINARY, help="Path to m1nd-mcp.")
    parser.add_argument(
        "--binary-args",
        default=" ".join(DEFAULT_ARGS),
        help='Arguments for the binary, default: "--stdio --no-gui".',
    )

    subparsers = parser.add_subparsers(dest="command", required=True)

    tools_parser = subparsers.add_parser("tools", help="List the live tool surface.")
    tools_parser.add_argument(
        "--names-only", action="store_true", help="Print one tool name per line."
    )

    call_parser = subparsers.add_parser("call", help="Call one m1nd tool.")
    call_parser.add_argument("tool_name", help="Canonical tool name, e.g. health.")
    call_parser.add_argument(
        "arguments_json",
        nargs="?",
        default="{}",
        help='JSON object with tool arguments, e.g. \'{"agent_id":"codex"}\'.',
    )

    run_parser = subparsers.add_parser(
        "run", help="Run multiple tool calls against the same m1nd process."
    )
    run_parser.add_argument(
        "steps_json",
        help=(
            "JSON array of step objects, e.g. "
            '\'[{"name":"ingest","arguments":{"agent_id":"codex","path":"/repo"}}]\''
        ),
    )

    args = parser.parse_args()

    binary_args = shlex.split(args.binary_args)
    client = McpClient(args.binary, binary_args)
    try:
        if args.command == "tools":
            response = client.tools()
            tools = response["result"]["tools"]
            if args.names_only:
                for tool in tools:
                    print(tool["name"])
                return 0
            print_json({"count": len(tools), "names": [tool["name"] for tool in tools]})
            return 0

        if args.command == "call":
            try:
                tool_arguments = json.loads(args.arguments_json)
            except json.JSONDecodeError as exc:
                raise SystemExit(f"invalid arguments_json: {exc}") from exc
            response = client.call(args.tool_name, tool_arguments)
            result = response.get("result", {})
            content = result.get("content", [])
            if content and content[0].get("type") == "text":
                parsed = parse_embedded_json(content[0].get("text", ""))
                output = {"isError": result.get("isError", False), "payload": parsed}
                print_json(output)
                return 0
            print_json(response)
            return 0

        if args.command == "run":
            try:
                steps = json.loads(args.steps_json)
            except json.JSONDecodeError as exc:
                raise SystemExit(f"invalid steps_json: {exc}") from exc
            if not isinstance(steps, list):
                raise SystemExit("steps_json must be a JSON array")

            outputs = []
            for index, step in enumerate(steps, start=1):
                if not isinstance(step, dict):
                    raise SystemExit(f"step {index} must be an object")
                tool_name = step.get("name")
                tool_arguments = step.get("arguments", {})
                if not isinstance(tool_name, str):
                    raise SystemExit(f"step {index} is missing string field 'name'")
                if not isinstance(tool_arguments, dict):
                    raise SystemExit(
                        f"step {index} field 'arguments' must be a JSON object"
                    )
                response = client.call(tool_name, tool_arguments)
                result = response.get("result", {})
                content = result.get("content", [])
                payload: Any = response
                if content and content[0].get("type") == "text":
                    payload = {
                        "isError": result.get("isError", False),
                        "payload": parse_embedded_json(content[0].get("text", "")),
                    }
                outputs.append({"step": index, "name": tool_name, "result": payload})
            print_json(outputs)
            return 0

        raise SystemExit(f"unsupported command: {args.command}")
    finally:
        client.close()


if __name__ == "__main__":
    sys.exit(main())
