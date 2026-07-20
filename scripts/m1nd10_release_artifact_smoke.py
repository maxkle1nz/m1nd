#!/usr/bin/env python3
"""Cross-platform smoke for an already-built M1ND release archive.

This harness never builds the binary. It proves that the exact candidate bytes
declare the expected source identity, refuse a non-loopback bind, serve
authenticated health/manifest endpoints, reject an unauthenticated request, and
accept a real stdio-to-HTTP attach initialization.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import queue
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any


SCHEMA = "m1nd-release-artifact-smoke-v1"


class SmokeError(RuntimeError):
    pass


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def free_loopback_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def request_json(
    url: str, token: str | None = None, *, timeout: float = 2.0
) -> tuple[int, Any, bytes]:
    headers = {"Accept": "application/json"}
    if token is not None:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            raw = response.read()
            return response.status, json.loads(raw), raw
    except urllib.error.HTTPError as error:
        raw = error.read()
        try:
            body: Any = json.loads(raw)
        except json.JSONDecodeError:
            body = raw.decode("utf-8", "replace")
        return error.code, body, raw


def request_bytes(
    url: str, token: str | None = None, *, timeout: float = 2.0
) -> tuple[int, bytes]:
    headers = {"Accept": "text/html,application/octet-stream"}
    if token is not None:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.read()


def wait_for_owner(
    *, base_url: str, token_path: Path, process: subprocess.Popen[bytes], deadline: float
) -> tuple[str, dict[str, Any], bytes]:
    last_error = "owner did not answer"
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise SmokeError(f"owner exited before readiness with code {process.returncode}")
        if token_path.is_file():
            token = token_path.read_text(encoding="utf-8").strip()
            if token:
                try:
                    status, health, raw = request_json(f"{base_url}/api/health", token)
                    if status == 200 and isinstance(health, dict):
                        return token, health, raw
                    last_error = f"health status={status}"
                except (OSError, ValueError) as error:
                    last_error = str(error)
        time.sleep(0.1)
    raise SmokeError(f"owner readiness timeout: {last_error}")


def validate_version(output: str, expected_version: str, expected_commit: str) -> None:
    if expected_version not in output:
        raise SmokeError(
            f"binary version output does not contain {expected_version!r}: {output!r}"
        )
    if expected_commit[:7] not in output:
        raise SmokeError(
            f"binary version output does not bind commit {expected_commit[:7]!r}: {output!r}"
        )


def validate_ui_manifest(manifest: dict[str, Any], expected_sha256: str) -> dict[str, str]:
    if not isinstance(expected_sha256, str) or len(expected_sha256) != 64:
        raise SmokeError("expected UI digest must be a raw 64-character SHA-256")
    sealed = manifest.get("manifest")
    if not isinstance(sealed, dict):
        raise SmokeError("organism manifest body is absent")
    ui = sealed.get("ui")
    authorities = sealed.get("authorities")
    authority = authorities.get("ui_bundle") if isinstance(authorities, dict) else None
    if not isinstance(ui, dict) or not isinstance(authority, dict):
        raise SmokeError("organism manifest has no UI authority binding")
    expected = f"sha256:{expected_sha256}"
    if ui.get("bundle_sha256") != expected or authority.get("digest") != expected:
        raise SmokeError("embedded UI digest does not match the rebuilt workflow artifact")
    if ui.get("mode") != "embedded":
        raise SmokeError(f"release binary UI mode is not embedded: {ui.get('mode')!r}")
    if authority.get("status") != "AVAILABLE" or authority.get("freshness") != "FRESH":
        raise SmokeError(
            "embedded UI authority is not AVAILABLE/FRESH: "
            f"status={authority.get('status')!r}, freshness={authority.get('freshness')!r}"
        )
    return {
        "freshness": authority["freshness"],
        "mode": ui["mode"],
        "sha256": expected_sha256,
        "status": authority["status"],
    }


def attach_initialize(
    *, binary: Path, base_url: str, runtime_dir: Path, project_dir: Path, timeout: float
) -> dict[str, Any]:
    frame = {
        "jsonrpc": "2.0",
        "id": "artifact-smoke-init",
        "method": "initialize",
        "params": {
            "capabilities": {},
            "clientInfo": {"name": "m1nd-release-artifact-smoke", "version": "1"},
            "protocolVersion": "2025-03-26",
        },
    }
    command = [
        str(binary),
        "--attach",
        base_url,
        "--runtime-dir",
        str(runtime_dir),
        "--no-gui",
    ]
    process = subprocess.Popen(
        command,
        cwd=project_dir,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert process.stdin is not None
    assert process.stdout is not None
    assert process.stderr is not None
    lines: queue.Queue[bytes | None] = queue.Queue()
    stderr_chunks: list[bytes] = []

    def read_stdout() -> None:
        for line in iter(process.stdout.readline, b""):
            lines.put(line)
        lines.put(None)

    def read_stderr() -> None:
        for chunk in iter(lambda: process.stderr.read(4096), b""):
            stderr_chunks.append(chunk)

    threading.Thread(target=read_stdout, daemon=True).start()
    threading.Thread(target=read_stderr, daemon=True).start()
    process.stdin.write((json.dumps(frame, separators=(",", ":")) + "\n").encode())
    process.stdin.flush()
    deadline = time.monotonic() + timeout
    messages: list[dict[str, Any]] = []
    try:
        while time.monotonic() < deadline:
            try:
                line = lines.get(timeout=min(0.2, max(0.01, deadline - time.monotonic())))
            except queue.Empty:
                if process.poll() is not None:
                    break
                continue
            if line is None:
                break
            try:
                value = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(value, dict):
                messages.append(value)
                if value.get("id") == "artifact-smoke-init":
                    if "result" in value and "error" not in value:
                        return value
                    break
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
    detail = b"".join(stderr_chunks).decode("utf-8", "replace")[-2000:]
    raise SmokeError(
        "attach returned no successful initialize response: "
        f"messages={messages!r}, code={process.returncode}, stderr={detail}"
    )


def non_loopback_refusal(binary: Path, root: Path, timeout: float) -> None:
    command = [
        str(binary),
        "--serve",
        "--bind",
        "0.0.0.0",
        "--port",
        str(free_loopback_port()),
        "--runtime-dir",
        str(root / "remote-runtime"),
        "--registry-dir",
        str(root / "remote-registry"),
        "--no-gui",
    ]
    result = subprocess.run(
        command,
        cwd=root,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    combined = (result.stdout + result.stderr).decode("utf-8", "replace").lower()
    if result.returncode == 0 or "refus" not in combined:
        raise SmokeError(
            "non-loopback serve was not refused explicitly: "
            f"code={result.returncode}, output={combined[-2000:]!r}"
        )


def run(args: argparse.Namespace) -> dict[str, Any]:
    binary = args.binary.resolve()
    if not binary.is_file():
        raise SmokeError(f"binary does not exist: {binary}")
    version = subprocess.run(
        [str(binary), "--version"],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=args.timeout,
        check=True,
    ).stdout.decode("utf-8", "replace").strip()
    validate_version(version, args.expected_version, args.expected_commit)

    with tempfile.TemporaryDirectory(prefix="m1nd-release-smoke-") as temporary:
        root = Path(temporary)
        non_loopback_refusal(binary, root, args.timeout)
        project = root / "repo-alpha"
        project.mkdir()
        (project / "main.py").write_text("def release_smoke():\n    return True\n")
        runtime = root / "runtime"
        registry = root / "registry"
        port = free_loopback_port()
        base_url = f"http://127.0.0.1:{port}"
        log_path = root / "owner.log"
        command = [
            str(binary),
            "--serve",
            "--bind",
            "127.0.0.1",
            "--port",
            str(port),
            "--runtime-dir",
            str(runtime),
            "--registry-dir",
            str(registry),
            "--no-gui",
        ]
        with log_path.open("w+b") as log:
            owner = subprocess.Popen(
                command,
                cwd=project,
                stdin=subprocess.DEVNULL,
                stdout=log,
                stderr=subprocess.STDOUT,
            )
            try:
                token, health, health_raw = wait_for_owner(
                    base_url=base_url,
                    token_path=runtime / "http-auth-token-v1",
                    process=owner,
                    deadline=time.monotonic() + args.timeout,
                )
                unauth_status, _, _ = request_json(f"{base_url}/api/manifest")
                if unauth_status != 401:
                    raise SmokeError(
                        f"unauthenticated manifest expected 401, got {unauth_status}"
                    )
                manifest_status, manifest, manifest_raw = request_json(
                    f"{base_url}/api/manifest", token, timeout=args.timeout
                )
                if manifest_status != 200 or not isinstance(manifest, dict):
                    raise SmokeError(f"authenticated manifest status={manifest_status}")
                if (
                    manifest.get("schema") != "m1nd-organism-manifest-response-v1"
                    or manifest.get("manifest", {}).get("schema")
                    != "m1nd-organism-manifest-v1"
                ):
                    raise SmokeError("authenticated manifest returned an unexpected schema")
                sealed_digest = manifest.get("manifest", {}).get("manifest_sha256")
                computed_digest = manifest.get("verification", {}).get(
                    "computed_manifest_sha256"
                )
                if (
                    not isinstance(sealed_digest, str)
                    or not sealed_digest
                    or sealed_digest != computed_digest
                ):
                    raise SmokeError("manifest self digest did not verify")
                ui_bundle = validate_ui_manifest(manifest, args.expected_ui_sha256)
                index_status, index_body = request_bytes(
                    f"{base_url}/", token, timeout=args.timeout
                )
                if (
                    index_status != 200
                    or not index_body
                    or b"m1nd UI not built" in index_body
                ):
                    raise SmokeError(
                        "embedded UI index was absent, empty, or a placeholder: "
                        f"status={index_status}, size={len(index_body)}"
                    )
                attach = attach_initialize(
                    binary=binary,
                    base_url=base_url,
                    runtime_dir=runtime,
                    project_dir=project,
                    timeout=args.timeout,
                )
            finally:
                owner.terminate()
                try:
                    owner.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    owner.kill()
                    owner.wait(timeout=5)

        return {
            "schema": SCHEMA,
            "target": args.target,
            "binary": {
                "name": binary.name,
                "sha256": sha256_file(binary),
                "size_bytes": binary.stat().st_size,
                "version_output": version,
            },
            "expected": {
                "commit": args.expected_commit,
                "version": args.expected_version,
            },
            "environment": {
                "machine": platform.machine(),
                "os": platform.system(),
                "os_release": platform.release(),
            },
            "ui_bundle": ui_bundle,
            "proofs": {
                "attach_initialize": attach["id"] == "artifact-smoke-init",
                "authenticated_health_sha256": hashlib.sha256(health_raw).hexdigest(),
                "authenticated_manifest_sha256": hashlib.sha256(manifest_raw).hexdigest(),
                "manifest_coherence": manifest["verification"].get("coherence"),
                "manifest_self_digest_verified": True,
                "embedded_ui_index_served": True,
                "embedded_ui_workflow_digest_match": True,
                "non_loopback_refused": True,
                "unauthenticated_manifest_status": unauth_status,
            },
            "verdict": "PASS",
        }


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    result.add_argument("--binary", type=Path, required=True)
    result.add_argument("--expected-version", required=True)
    result.add_argument("--expected-commit", required=True)
    result.add_argument("--expected-ui-sha256", required=True)
    result.add_argument("--target", required=True)
    result.add_argument("--output", type=Path, required=True)
    result.add_argument("--timeout", type=float, default=120.0)
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        receipt = run(args)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(
            json.dumps(receipt, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )
    except (OSError, SmokeError, subprocess.SubprocessError, ValueError) as error:
        print(f"release artifact smoke refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
