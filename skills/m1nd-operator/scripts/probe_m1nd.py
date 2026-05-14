#!/usr/bin/env python3

import argparse
import json
import os
import shlex
import subprocess
import sys
import shutil
import tempfile
from typing import Any


def default_binary() -> str:
    env_binary = os.environ.get("M1ND_MCP_BINARY") or os.environ.get("M1ND_MCP_BIN")
    if env_binary:
        return env_binary

    binary_name = "m1nd-mcp.exe" if os.name == "nt" else "m1nd-mcp"
    managed_binary = os.path.expanduser(os.path.join("~", ".m1nd", "bin", binary_name))
    if os.path.exists(managed_binary) and os.access(managed_binary, os.X_OK):
        return managed_binary

    return shutil.which("m1nd-mcp") or "m1nd-mcp"


DEFAULT_BINARY = default_binary()
DEFAULT_ARGS = shlex.split(os.environ.get("M1ND_MCP_ARGS", "--stdio --no-gui"))


class McpClient:
    def __init__(
        self,
        binary: str,
        extra_args: list[str],
        *,
        cwd: str | None = None,
        env: dict[str, str] | None = None,
    ) -> None:
        self.proc = subprocess.Popen(
            [binary, *extra_args],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            cwd=cwd,
            env=env,
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


def binary_args_have_runtime_dir(binary_args: list[str]) -> bool:
    return any(arg == "--runtime-dir" or arg.startswith("--runtime-dir=") for arg in binary_args)


def binary_args_have_option(binary_args: list[str], option_name: str) -> bool:
    return any(arg == option_name or arg.startswith(f"{option_name}=") for arg in binary_args)


def runtime_dir_from_args(binary_args: list[str]) -> str | None:
    for index, arg in enumerate(binary_args):
        if arg.startswith("--runtime-dir="):
            return arg.split("=", 1)[1]
        if arg == "--runtime-dir" and index + 1 < len(binary_args):
            return binary_args[index + 1]
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description="Probe the local m1nd MCP runtime.")
    parser.add_argument("--binary", default=DEFAULT_BINARY, help="Path to m1nd-mcp.")
    parser.add_argument(
        "--binary-args",
        default=" ".join(DEFAULT_ARGS),
        help='Arguments for the binary, default: "--stdio --no-gui".',
    )
    parser.add_argument(
        "--runtime-dir",
        help="Runtime directory for isolated sidecar state. Defaults to a temporary directory.",
    )
    parser.add_argument(
        "--shared-runtime",
        action="store_true",
        help="Do not inject an isolated --runtime-dir. Use only when debugging shared runtime state.",
    )
    parser.add_argument(
        "--keep-runtime-dir",
        action="store_true",
        help="Keep the temporary runtime directory for debugging.",
    )
    parser.add_argument(
        "--no-worktree-artifacts",
        action="store_true",
        help=(
            "Launch m1nd-mcp from the isolated runtime directory so probe state "
            "does not materialize in the caller worktree. M1ND_WORKSPACE_ROOT "
            "is set from --workspace-root or the caller cwd."
        ),
    )
    parser.add_argument(
        "--workspace-root",
        help=(
            "Workspace root to expose to m1nd when --no-worktree-artifacts is set. "
            "Defaults to the caller cwd."
        ),
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
    if args.runtime_dir and args.shared_runtime:
        raise SystemExit("--runtime-dir and --shared-runtime are mutually exclusive")
    if args.no_worktree_artifacts and args.shared_runtime:
        raise SystemExit("--no-worktree-artifacts cannot be combined with --shared-runtime")

    runtime_dir_created = None
    runtime_dir = runtime_dir_from_args(binary_args)
    if not binary_args_have_runtime_dir(binary_args) and not args.shared_runtime:
        runtime_dir = (
            os.path.abspath(os.path.expanduser(args.runtime_dir))
            if args.runtime_dir
            else tempfile.mkdtemp(prefix="m1nd-probe-")
        )
        os.makedirs(runtime_dir, exist_ok=True)
        if not args.runtime_dir:
            runtime_dir_created = runtime_dir
        binary_args.extend(["--runtime-dir", runtime_dir])
    elif runtime_dir:
        runtime_dir = os.path.abspath(os.path.expanduser(runtime_dir))

    launch_cwd = None
    launch_env = None
    if args.no_worktree_artifacts:
        if not runtime_dir:
            raise SystemExit("--no-worktree-artifacts requires an isolated runtime directory")
        os.makedirs(runtime_dir, exist_ok=True)
        launch_env = dict(os.environ)
        workspace_root = os.path.abspath(
            os.path.expanduser(args.workspace_root or os.getcwd())
        )
        launch_env["M1ND_WORKSPACE_ROOT"] = workspace_root
        launch_env.setdefault("M1ND_RUNTIME_BASE", runtime_dir)
        launch_cwd = runtime_dir
        if not binary_args_have_option(binary_args, "--graph"):
            launch_env.setdefault(
                "M1ND_GRAPH_SOURCE", os.path.join(runtime_dir, "graph_snapshot.json")
            )
        if not binary_args_have_option(binary_args, "--plasticity"):
            launch_env.setdefault(
                "M1ND_PLASTICITY_STATE",
                os.path.join(runtime_dir, "plasticity_state.json"),
            )

    client = None
    try:
        client = McpClient(args.binary, binary_args, cwd=launch_cwd, env=launch_env)
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
        if client is not None:
            client.close()
        if runtime_dir_created and not args.keep_runtime_dir:
            shutil.rmtree(runtime_dir_created, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
