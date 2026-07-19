#!/usr/bin/env python3
"""Run the opt-in G7 browser gate against one explicit, isolated M1ND binary.

This launcher never discovers, attaches to, stops, or probes an installed M1ND
service.  It verifies exact source/binary/UI identities, boots one read-only
owner on a kernel-selected or preflighted numeric-loopback port with ephemeral runtime and registry
roots, invokes the existing ``npm run test:e2e:live`` lane, emits a token-free
receipt, and terminates every process group it created.
"""

from __future__ import annotations

import argparse
import hashlib
import http.client
import io
import json
import math
import os
import re
import shutil
import signal
import socket
import stat
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import tomllib
from contextlib import ExitStack
from pathlib import Path, PurePosixPath
from typing import Any, BinaryIO


SCHEMA = "m1nd10-g7-live-orchestrator-receipt-v1"
OWNER_RESPONSE_SCHEMA = "m1nd-organism-manifest-response-v1"
MANIFEST_SCHEMA = "m1nd-organism-manifest-v1"
UI_TREE_DOMAIN = b"m1nd-ui-bundle-tree-v1\0"
HARNESS_TREE_DOMAIN = b"m1nd-g7-harness-tree-v1\0"
DEPENDENCY_TREE_DOMAIN = b"m1nd-g7-node-modules-tree-v1\0"
BROWSER_TREE_DOMAIN = b"m1nd-g7-browser-bundle-tree-v1\0"
NPM_TREE_DOMAIN = b"m1nd-g7-npm-tree-v1\0"
ROOT_FINGERPRINT_PREFIX = b"m1nd-domain-separated-sha256-v1\0"
ROOT_FINGERPRINT_DOMAIN = b"m1nd-project-root-fingerprint-v1"
INSTALLED_OWNER_PORT = 1338
TOKEN_FILE_NAME = "http-auth-token-v1"
MAX_HTTP_BODY = 16 * 1024 * 1024
SAFE_ENV_KEYS = ("HOME", "LANG", "LC_ALL", "PATH", "TMPDIR")
HEX_SHA256 = re.compile(r"[0-9a-f]{64}")
HEX_GIT_OBJECT = re.compile(r"[0-9a-f]{40}(?:[0-9a-f]{24})?")
NPM_INTEGRITY = re.compile(r"sha512-[A-Za-z0-9+/]+={0,2}")


class G7OrchestratorError(RuntimeError):
    """Closed failure with a stable receipt code and stage."""

    def __init__(self, stage: str, code: str, detail: str):
        super().__init__(detail)
        self.stage = stage
        self.code = code
        self.detail = detail


def refuse(stage: str, code: str, detail: str) -> None:
    raise G7OrchestratorError(stage, code, detail)


def normalize_sha256(value: str, label: str) -> str:
    raw = value.strip().removeprefix("sha256:")
    if HEX_SHA256.fullmatch(raw) is None:
        refuse(
            "input_validation", "invalid_sha256", f"{label} must be lowercase SHA-256"
        )
    return raw


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def framed_tree_identity(
    root: Path,
    domain: bytes,
    *,
    label: str,
    exclude_top_level: frozenset[str] = frozenset(),
) -> tuple[str, int, int]:
    """Hash regular files and safe in-tree symlinks without following links."""

    root = absolute_directory(root, label)
    entries: list[tuple[Path, os.stat_result]] = []
    try:
        for current, directory_names, file_names in os.walk(
            root, topdown=True, followlinks=False
        ):
            current_path = Path(current)
            relative_current = current_path.relative_to(root)
            if (
                relative_current.parts
                and relative_current.parts[0] in exclude_top_level
            ):
                directory_names[:] = []
                continue
            for name in list(directory_names):
                path = current_path / name
                relative = path.relative_to(root)
                if relative.parts[0] in exclude_top_level:
                    directory_names.remove(name)
                    continue
                path_stat = path.lstat()
                if stat.S_ISLNK(path_stat.st_mode):
                    directory_names.remove(name)
                    entries.append((path, path_stat))
                elif not stat.S_ISDIR(path_stat.st_mode):
                    refuse(
                        "input_identity",
                        "tree_entry_unsafe",
                        f"{label} contains a non-directory traversal entry",
                    )
            for name in file_names:
                path = current_path / name
                relative = path.relative_to(root)
                if relative.parts[0] in exclude_top_level:
                    continue
                path_stat = path.lstat()
                if not (
                    stat.S_ISREG(path_stat.st_mode) or stat.S_ISLNK(path_stat.st_mode)
                ):
                    refuse(
                        "input_identity",
                        "tree_entry_unsafe",
                        f"{label} contains a special file",
                    )
                entries.append((path, path_stat))
    except OSError:
        refuse("input_identity", "tree_unreadable", f"{label} could not be walked")

    entries.sort(key=lambda item: item[0].relative_to(root).as_posix())
    digest = hashlib.sha256()
    digest.update(domain)
    byte_count = 0
    for path, before in entries:
        relative = path.relative_to(root).as_posix().encode("utf-8")
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        digest.update(stat.S_IMODE(before.st_mode).to_bytes(4, "big"))
        if stat.S_ISLNK(before.st_mode):
            try:
                target = os.readlink(path)
                target_bytes = os.fsencode(target)
                if os.path.isabs(target):
                    raise OSError("absolute link")
                resolved = (path.parent / target).resolve(strict=True)
                after = path.lstat()
            except OSError:
                refuse(
                    "input_identity",
                    "tree_symlink_unsafe",
                    f"{label} contains an unreadable, absolute, or broken symlink",
                )
            if not is_within(resolved, root) or (
                before.st_dev,
                before.st_ino,
                before.st_mtime_ns,
            ) != (after.st_dev, after.st_ino, after.st_mtime_ns):
                refuse(
                    "input_identity",
                    "tree_symlink_unsafe",
                    f"{label} contains an escaping or changing symlink",
                )
            digest.update(b"L")
            digest.update(len(target_bytes).to_bytes(8, "big"))
            digest.update(target_bytes)
            byte_count += len(target_bytes)
            continue

        descriptor: int | None = None
        try:
            descriptor = os.open(
                path,
                os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0),
            )
            opened = os.fstat(descriptor)
            if not stat.S_ISREG(opened.st_mode) or (
                before.st_dev,
                before.st_ino,
                before.st_size,
            ) != (opened.st_dev, opened.st_ino, opened.st_size):
                raise OSError("identity changed")
            digest.update(b"F")
            digest.update(opened.st_size.to_bytes(8, "big"))
            while True:
                chunk = os.read(descriptor, 1024 * 1024)
                if not chunk:
                    break
                digest.update(chunk)
            after = os.fstat(descriptor)
            if (
                opened.st_size,
                opened.st_mtime_ns,
                opened.st_ctime_ns,
            ) != (after.st_size, after.st_mtime_ns, after.st_ctime_ns):
                raise OSError("contents changed")
            byte_count += opened.st_size
        except OSError:
            refuse(
                "input_identity",
                "tree_file_changed",
                f"{label} contains an unreadable or changing file",
            )
        finally:
            if descriptor is not None:
                os.close(descriptor)
    return digest.hexdigest(), len(entries), byte_count


def is_within(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def absolute_regular_file(path: Path, label: str, *, executable: bool = False) -> Path:
    if not path.is_absolute():
        refuse("input_validation", "relative_path", f"{label} must be absolute")
    try:
        path_stat = path.lstat()
    except OSError:
        refuse("input_validation", "missing_path", f"{label} does not exist")
    if stat.S_ISLNK(path_stat.st_mode) or not stat.S_ISREG(path_stat.st_mode):
        refuse(
            "input_validation",
            "unsafe_path_topology",
            f"{label} must be a regular non-symlink file",
        )
    canonical = path.resolve(strict=True)
    if executable and not os.access(canonical, os.X_OK):
        refuse(
            "input_validation", "binary_not_executable", f"{label} is not executable"
        )
    return canonical


def absolute_directory(path: Path, label: str) -> Path:
    if not path.is_absolute():
        refuse("input_validation", "relative_path", f"{label} must be absolute")
    try:
        path_stat = path.lstat()
    except OSError:
        refuse("input_validation", "missing_path", f"{label} does not exist")
    if stat.S_ISLNK(path_stat.st_mode) or not stat.S_ISDIR(path_stat.st_mode):
        refuse(
            "input_validation",
            "unsafe_path_topology",
            f"{label} must be a directory and not a symlink",
        )
    return path.resolve(strict=True)


def validate_output_path(path: Path) -> Path:
    if not path.is_absolute():
        refuse("input_validation", "relative_output", "--output must be absolute")
    parent = absolute_directory(path.parent, "--output parent")
    canonical = parent / path.name
    if canonical.exists() or canonical.is_symlink():
        try:
            existing = canonical.lstat()
        except OSError:
            refuse(
                "input_validation", "unsafe_output", "--output could not be inspected"
            )
        if stat.S_ISLNK(existing.st_mode) or not stat.S_ISREG(existing.st_mode):
            refuse(
                "input_validation",
                "unsafe_output",
                "--output must be absent or a regular non-symlink file",
            )
    return canonical


def validate_static_topology(binary: Path, source_root: Path, output: Path) -> None:
    installed_root = (Path.home() / ".m1nd").resolve(strict=False)
    if is_within(binary, installed_root):
        refuse(
            "input_validation",
            "installed_binary_refused",
            "the supplied binary is under the installed M1ND state root",
        )
    if is_within(source_root, installed_root):
        refuse(
            "input_validation",
            "installed_source_refused",
            "the supplied source root is under the installed M1ND state root",
        )
    if is_within(output, source_root):
        refuse(
            "input_validation",
            "receipt_inside_source_refused",
            "the receipt must be outside the source root so proof does not dirty its subject",
        )
    if output == binary:
        refuse(
            "input_validation",
            "receipt_over_binary_refused",
            "receipt would overwrite binary",
        )


def clean_subprocess_env() -> dict[str, str]:
    return {key: os.environ[key] for key in SAFE_ENV_KEYS if os.environ.get(key)}


def git_output(source_root: Path, *arguments: str) -> str:
    try:
        result = subprocess.run(
            ["git", "-C", str(source_root), *arguments],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            env=clean_subprocess_env(),
            timeout=15,
        )
    except (OSError, subprocess.TimeoutExpired):
        refuse("source_validation", "git_unavailable", "git identity could not be read")
    if result.returncode != 0:
        refuse(
            "source_validation",
            "git_identity_failed",
            "source root has no readable git identity",
        )
    return result.stdout.decode("utf-8", "replace").strip()


def ui_tree_identity(root: Path) -> tuple[str, int]:
    root = absolute_directory(root, "source UI dist")
    files: list[Path] = []
    for entry in root.rglob("*"):
        entry_stat = entry.lstat()
        if stat.S_ISLNK(entry_stat.st_mode):
            refuse(
                "source_validation",
                "ui_symlink_refused",
                "source UI dist contains a symlink",
            )
        if stat.S_ISREG(entry_stat.st_mode):
            files.append(entry)
    files.sort(key=lambda path: path.relative_to(root).as_posix())
    if not files or not (root / "index.html").is_file():
        refuse(
            "source_validation",
            "ui_dist_incomplete",
            "source UI dist is empty or has no index",
        )
    digest = hashlib.sha256()
    digest.update(UI_TREE_DOMAIN)
    for path in files:
        relative = path.relative_to(root).as_posix().encode("utf-8")
        payload = path.read_bytes()
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        digest.update(len(payload).to_bytes(8, "big"))
        digest.update(payload)
    return digest.hexdigest(), len(files)


def source_version(source_root: Path) -> str:
    manifest_path = absolute_regular_file(
        source_root / "m1nd-mcp" / "Cargo.toml", "m1nd-mcp/Cargo.toml"
    )
    try:
        value = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        version = value["package"]["version"]
    except (OSError, KeyError, TypeError, tomllib.TOMLDecodeError):
        refuse(
            "source_validation",
            "source_version_unreadable",
            "M1ND source version is unreadable",
        )
    if not isinstance(version, str) or not version.strip():
        refuse(
            "source_validation",
            "source_version_invalid",
            "M1ND source version is empty",
        )
    return version.strip()


def inspect_source(source_root: Path, expected_ui_sha256: str) -> dict[str, Any]:
    for required in (
        source_root / "Cargo.toml",
        source_root / "m1nd-ui" / "package.json",
        source_root / "m1nd-ui" / "package-lock.json",
        source_root / "m1nd-ui" / "playwright.live.config.ts",
        source_root / "m1nd-ui" / "e2e-live" / "live-config.ts",
        source_root / "m1nd-ui" / "e2e-live" / "live-test.ts",
        source_root / "m1nd-ui" / "e2e-live" / "live-owner.spec.ts",
        source_root / "graph_snapshot.json",
    ):
        absolute_regular_file(required, str(required.relative_to(source_root)))
    git_root = Path(git_output(source_root, "rev-parse", "--show-toplevel")).resolve(
        strict=True
    )
    if git_root != source_root:
        refuse(
            "source_validation",
            "source_root_mismatch",
            "--source-root is not the exact git worktree root",
        )
    head = git_output(source_root, "rev-parse", "HEAD")
    tree = git_output(source_root, "rev-parse", "HEAD^{tree}")
    if HEX_GIT_OBJECT.fullmatch(head) is None or HEX_GIT_OBJECT.fullmatch(tree) is None:
        refuse(
            "source_validation",
            "git_object_invalid",
            "source git object identity is invalid",
        )
    if git_output(source_root, "status", "--porcelain"):
        refuse(
            "source_validation",
            "source_dirty",
            "source worktree is dirty; a coherent G7 source authority is impossible",
        )
    ui_sha256, ui_file_count = ui_tree_identity(source_root / "m1nd-ui" / "dist")
    if ui_sha256 != expected_ui_sha256:
        refuse(
            "source_validation",
            "source_ui_digest_mismatch",
            "source UI dist differs from the explicitly expected promoted bundle",
        )
    return {
        "commit": head,
        "tree": tree,
        "version": source_version(source_root),
        "ui_dist_sha256": ui_sha256,
        "ui_file_count": ui_file_count,
        "ui_package_lock_sha256": sha256_file(
            source_root / "m1nd-ui" / "package-lock.json"
        ),
    }


def stage_git_ui_tree(source_root: Path, commit: str, destination: Path) -> Path:
    """Materialize only tracked UI bytes from the exact commit into fresh state."""

    workspace = destination / "ui-harness"
    if workspace.exists() or workspace.is_symlink():
        refuse(
            "dependency_preparation",
            "staging_not_empty",
            "ephemeral UI harness destination already exists",
        )
    try:
        archive = subprocess.run(
            [
                "git",
                "-C",
                str(source_root),
                "archive",
                "--format=tar",
                commit,
                "m1nd-ui",
            ],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            env=clean_subprocess_env(),
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired):
        refuse(
            "dependency_preparation",
            "git_archive_failed",
            "exact tracked UI harness could not be materialized",
        )
    if (
        archive.returncode != 0
        or not archive.stdout
        or len(archive.stdout) > 256 * 1024 * 1024
    ):
        refuse(
            "dependency_preparation",
            "git_archive_failed",
            "exact tracked UI harness archive is absent or invalid",
        )

    destination.mkdir(mode=0o700)
    try:
        with tarfile.open(fileobj=io.BytesIO(archive.stdout), mode="r:") as bundle:
            for member in bundle.getmembers():
                member_path = PurePosixPath(member.name)
                if (
                    member_path.is_absolute()
                    or not member_path.parts
                    or member_path.parts[0] != "m1nd-ui"
                    or any(part in {"", ".", ".."} for part in member_path.parts)
                ):
                    raise ValueError("unsafe archive path")
                relative = Path(*member_path.parts[1:])
                target = workspace / relative
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True, mode=0o700)
                    continue
                if not member.isreg() or member.size > 64 * 1024 * 1024:
                    raise ValueError("unsafe archive entry")
                payload = bundle.extractfile(member)
                if payload is None:
                    raise ValueError("missing archive payload")
                target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
                with target.open("xb") as handle:
                    shutil.copyfileobj(payload, handle, length=1024 * 1024)
                os.chmod(target, member.mode & 0o777 or 0o600)
    except (OSError, tarfile.TarError, ValueError):
        refuse(
            "dependency_preparation",
            "git_archive_unsafe",
            "tracked UI archive contained an unsafe or unreadable entry",
        )

    for required in (
        workspace / "package.json",
        workspace / "package-lock.json",
        workspace / "playwright.live.config.ts",
        workspace / "e2e-live" / "live-config.ts",
        workspace / "e2e-live" / "live-test.ts",
        workspace / "e2e-live" / "live-owner.spec.ts",
    ):
        absolute_regular_file(required, f"staged {required.relative_to(workspace)}")
    if (workspace / "node_modules").exists() or (
        workspace / "node_modules"
    ).is_symlink():
        refuse(
            "dependency_preparation",
            "preexisting_node_modules",
            "tracked UI archive unexpectedly contains node_modules",
        )
    return workspace.resolve(strict=True)


def validate_locked_ui_dependencies(workspace: Path) -> dict[str, Any]:
    package_path = absolute_regular_file(
        workspace / "package.json", "staged package.json"
    )
    lock_path = absolute_regular_file(
        workspace / "package-lock.json", "staged package-lock.json"
    )
    try:
        package = json.loads(package_path.read_bytes())
        lock = json.loads(lock_path.read_bytes())
        packages = lock["packages"]
        root = packages[""]
    except (OSError, UnicodeError, json.JSONDecodeError, KeyError, TypeError):
        refuse(
            "dependency_preparation",
            "package_lock_invalid",
            "staged package or lock is unreadable",
        )
    if (
        not isinstance(package, dict)
        or not isinstance(lock, dict)
        or lock.get("lockfileVersion") != 3
        or not isinstance(packages, dict)
        or not isinstance(root, dict)
        or package.get("name") != root.get("name")
        or package.get("version") != root.get("version")
    ):
        refuse(
            "dependency_preparation",
            "package_lock_root_mismatch",
            "package-lock root does not exactly bind package.json",
        )
    dependency_count = 0
    for path, entry in packages.items():
        if path == "":
            continue
        dependency_count += 1
        if (
            not isinstance(path, str)
            or not path.startswith("node_modules/")
            or not isinstance(entry, dict)
            or entry.get("link") is True
            or not isinstance(entry.get("resolved"), str)
            or not entry["resolved"].startswith("https://registry.npmjs.org/")
            or not isinstance(entry.get("integrity"), str)
            or NPM_INTEGRITY.fullmatch(entry["integrity"]) is None
        ):
            refuse(
                "dependency_preparation",
                "package_lock_open_dependency",
                "every UI dependency must be registry-pinned with SHA-512 integrity",
            )
    if dependency_count == 0:
        refuse(
            "dependency_preparation",
            "package_lock_empty",
            "UI package lock has no dependencies",
        )
    playwright = packages.get("node_modules/playwright")
    playwright_core = packages.get("node_modules/playwright-core")
    if not isinstance(playwright, dict) or not isinstance(playwright_core, dict):
        refuse(
            "dependency_preparation",
            "playwright_unlocked",
            "Playwright and playwright-core must both be present in the closed lock",
        )
    return {
        "package_json_sha256": sha256_file(package_path),
        "package_lock_sha256": sha256_file(lock_path),
        "dependency_count": dependency_count,
        "playwright_version": playwright.get("version"),
        "playwright_core_version": playwright_core.get("version"),
    }


def reserve_loopback_port(requested_port: int | None) -> tuple[socket.socket, int]:
    port = 0 if requested_port is None else requested_port
    if port == INSTALLED_OWNER_PORT:
        refuse(
            "input_validation",
            "installed_port_refused",
            "port 1338 is reserved for the installed owner and is outside this gate",
        )
    if (
        not isinstance(port, int)
        or isinstance(port, bool)
        or (requested_port is not None and port < 1)
        or port > 65_535
    ):
        refuse("input_validation", "invalid_port", "--port must be between 1 and 65535")
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        listener.bind(("127.0.0.1", port))
        selected = int(listener.getsockname()[1])
    except OSError:
        listener.close()
        refuse(
            "input_validation",
            "port_unavailable",
            "requested loopback port is unavailable",
        )
    if selected == INSTALLED_OWNER_PORT:
        listener.close()
        refuse(
            "input_validation",
            "installed_port_refused",
            "automatic selection returned port 1338",
        )
    return listener, selected


def stable_copy(source: Path, destination: Path, label: str) -> str:
    source = absolute_regular_file(source, label)
    before = sha256_file(source)
    shutil.copyfile(source, destination)
    os.chmod(destination, 0o600)
    after = sha256_file(source)
    copied = sha256_file(destination)
    if before != after or before != copied:
        refuse(
            "runtime_preparation",
            "source_changed_during_copy",
            f"{label} changed during copy",
        )
    return before


def prepare_ephemeral_runtime(source_root: Path, root: Path) -> dict[str, Any]:
    runtime = root / "runtime"
    registry = root / "registry"
    runtime.mkdir(mode=0o700)
    registry.mkdir(mode=0o700)
    graph = runtime / "graph_snapshot.json"
    plasticity = runtime / "plasticity_state.json"
    graph_sha256 = stable_copy(
        source_root / "graph_snapshot.json", graph, "graph_snapshot.json"
    )
    source_plasticity = source_root / "plasticity_state.json"
    plasticity_sha256: str | None = None
    if source_plasticity.exists() or source_plasticity.is_symlink():
        plasticity_sha256 = stable_copy(
            source_plasticity, plasticity, "plasticity_state.json"
        )
    ingest_roots = runtime / "ingest_roots.json"
    ingest_roots.write_text(json.dumps([str(source_root)]) + "\n", encoding="utf-8")
    os.chmod(ingest_roots, 0o600)
    return {
        "root": root,
        "runtime": runtime,
        "registry": registry,
        "graph": graph,
        "plasticity": plasticity,
        "graph_sha256": graph_sha256,
        "plasticity_sha256": plasticity_sha256,
    }


def owner_command(binary: Path, prepared: dict[str, Any], port: int) -> list[str]:
    if port == INSTALLED_OWNER_PORT:
        refuse(
            "owner_boot",
            "installed_port_refused",
            "owner command may not use port 1338",
        )
    return [
        str(binary),
        "--serve",
        "--bind",
        "127.0.0.1",
        "--port",
        str(port),
        "--graph",
        str(prepared["graph"]),
        "--plasticity",
        str(prepared["plasticity"]),
        "--runtime-dir",
        str(prepared["runtime"]),
        "--registry-dir",
        str(prepared["registry"]),
        "--read-only",
        "--no-gui",
    ]


def owner_environment(
    source_root: Path, prepared: dict[str, Any], source: dict[str, Any]
) -> dict[str, str]:
    environment = clean_subprocess_env()
    environment.update(
        {
            "M1ND_EXPECTED_SHA": source["commit"],
            "M1ND_EXPECTED_VERSION": source["version"],
            "M1ND_READ_ONLY": "1",
            "M1ND_RUNTIME_BASE": str(prepared["root"]),
            "M1ND_STRICT_VERSION": "1",
            "M1ND_WORKSPACE_ROOT": str(source_root),
        }
    )
    return environment


def assert_new_process_group(process: subprocess.Popen[bytes], label: str) -> int:
    try:
        group_id = os.getpgid(process.pid)
    except OSError:
        refuse(
            "process_isolation",
            "process_group_unreadable",
            f"{label} group could not be read",
        )
    if group_id != process.pid:
        refuse(
            "process_isolation",
            "process_group_not_isolated",
            f"{label} did not become its own process-group leader",
        )
    return group_id


def process_group_exists(group_id: int) -> bool:
    try:
        os.killpg(group_id, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def terminate_process_group(
    process: subprocess.Popen[bytes] | None,
    group_id: int | None,
    *,
    timeout: float = 5.0,
) -> bool:
    if process is None:
        return True
    if group_id is None or group_id != process.pid:
        # Fail-safe for the exceptional case where group identity could not be
        # established: stop and reap the known child, but report False because
        # descendant cleanup could not be proven.
        try:
            process.terminate()
            process.wait(timeout=timeout)
        except (OSError, subprocess.TimeoutExpired):
            try:
                process.kill()
                process.wait(timeout=timeout)
            except (OSError, subprocess.TimeoutExpired):
                pass
        return False
    try:
        os.killpg(group_id, signal.SIGTERM)
    except ProcessLookupError:
        pass
    deadline = time.monotonic() + timeout
    while process_group_exists(group_id) and time.monotonic() < deadline:
        # Reap an exited leader. On some POSIX hosts a zombie keeps killpg(..., 0)
        # observable until wait(), even though it can no longer receive signals.
        process.poll()
        time.sleep(0.02)
    if process_group_exists(group_id):
        try:
            os.killpg(group_id, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass
    try:
        process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        return False
    return not process_group_exists(group_id)


def read_private_token(path: Path) -> str:
    descriptor: int | None = None
    try:
        path_stat = path.lstat()
        if stat.S_ISLNK(path_stat.st_mode) or not stat.S_ISREG(path_stat.st_mode):
            refuse(
                "owner_readiness",
                "unsafe_token_file",
                "owner token is not a regular file",
            )
        flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
        descriptor = os.open(path, flags)
        opened = os.fstat(descriptor)
        if (opened.st_dev, opened.st_ino) != (path_stat.st_dev, path_stat.st_ino):
            refuse(
                "owner_readiness",
                "token_identity_changed",
                "owner token changed while opening",
            )
        if stat.S_IMODE(opened.st_mode) & 0o077:
            refuse(
                "owner_readiness",
                "token_permissions",
                "owner token permissions are not private",
            )
        if opened.st_size > 1024:
            refuse(
                "owner_readiness",
                "token_too_large",
                "owner token file is unexpectedly large",
            )
        payload = os.read(descriptor, 1025).decode("ascii", "strict").strip()
    except G7OrchestratorError:
        raise
    except (OSError, UnicodeError):
        refuse(
            "owner_readiness",
            "token_unreadable",
            "owner token could not be read safely",
        )
    finally:
        if descriptor is not None:
            os.close(descriptor)
    if HEX_SHA256.fullmatch(payload) is None:
        refuse(
            "owner_readiness",
            "token_invalid",
            "owner token is not canonical lowercase hex",
        )
    return payload


def read_private_json_file(path: Path, label: str) -> Any:
    descriptor: int | None = None
    try:
        path_stat = path.lstat()
        if stat.S_ISLNK(path_stat.st_mode) or not stat.S_ISREG(path_stat.st_mode):
            refuse(
                "owner_readiness",
                "unsafe_registry_entry",
                f"{label} is not a regular file",
            )
        descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
        opened = os.fstat(descriptor)
        if (opened.st_dev, opened.st_ino) != (path_stat.st_dev, path_stat.st_ino):
            refuse(
                "owner_readiness",
                "registry_identity_changed",
                f"{label} changed while opening",
            )
        if opened.st_size > 64 * 1024:
            refuse(
                "owner_readiness",
                "registry_entry_too_large",
                f"{label} is unexpectedly large",
            )
        payload = os.read(descriptor, 64 * 1024 + 1)
        return json.loads(payload)
    except G7OrchestratorError:
        raise
    except (OSError, UnicodeError, json.JSONDecodeError):
        raise ValueError(f"{label} is not ready") from None
    finally:
        if descriptor is not None:
            os.close(descriptor)


def wait_for_owned_endpoint(
    *,
    process: subprocess.Popen[bytes],
    registry: Path,
    requested_port: int | None,
    timeout: float,
) -> int:
    """Read only this launch's private registry; never inspect ambient sockets."""

    instances = registry / "instances"
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            refuse(
                "owner_readiness",
                "owner_exited",
                f"isolated owner exited before endpoint registration with code {process.returncode}",
            )
        if instances.is_symlink():
            refuse(
                "owner_readiness",
                "registry_instances_symlink",
                "ephemeral registry instances directory must not be a symlink",
            )
        try:
            entries = list(instances.iterdir()) if instances.is_dir() else []
        except OSError:
            entries = []
        for entry_path in entries:
            try:
                entry = read_private_json_file(entry_path, "ephemeral instance entry")
            except ValueError:
                continue
            if not isinstance(entry, dict) or entry.get("pid") != process.pid:
                continue
            if entry.get("mode") != "read_only":
                refuse(
                    "owner_readiness",
                    "registered_mode_invalid",
                    "isolated owner did not register read_only mode",
                )
            port = entry.get("port")
            bind = entry.get("bind")
            if bind is None and port is None:
                continue
            if (
                bind != "127.0.0.1"
                or not isinstance(port, int)
                or isinstance(port, bool)
                or port < 1
                or port > 65_535
            ):
                refuse(
                    "owner_readiness",
                    "registered_endpoint_invalid",
                    "isolated owner registered an invalid endpoint or mode",
                )
            if port == INSTALLED_OWNER_PORT:
                refuse(
                    "owner_readiness",
                    "installed_port_refused",
                    "isolated owner registered forbidden port 1338",
                )
            if requested_port is not None and port != requested_port:
                refuse(
                    "owner_readiness",
                    "registered_port_mismatch",
                    "isolated owner registered a different explicit port",
                )
            return port
        time.sleep(0.05)
    refuse(
        "owner_readiness",
        "endpoint_timeout",
        "isolated owner did not register its endpoint",
    )


def loopback_json(
    port: int, path: str, token: str, *, timeout: float
) -> tuple[int, Any, bytes]:
    if port == INSTALLED_OWNER_PORT:
        refuse(
            "owner_probe", "installed_port_refused", "no request may target port 1338"
        )
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    try:
        connection.request(
            "GET",
            path,
            headers={
                "Accept": "application/json",
                "Authorization": f"Bearer {token}",
                "Cache-Control": "no-store",
            },
        )
        response = connection.getresponse()
        payload = response.read(MAX_HTTP_BODY + 1)
    except (OSError, http.client.HTTPException):
        raise
    finally:
        connection.close()
    if len(payload) > MAX_HTTP_BODY:
        refuse(
            "owner_probe", "response_too_large", f"{path} exceeded the response limit"
        )
    try:
        value = json.loads(payload)
    except (UnicodeError, json.JSONDecodeError):
        refuse("owner_probe", "invalid_json", f"{path} did not return JSON")
    return int(response.status), value, payload


def wait_for_owner(
    process: subprocess.Popen[bytes], port: int, token_path: Path, timeout: float
) -> tuple[str, dict[str, Any]]:
    deadline = time.monotonic() + timeout
    last_state = "token_pending"
    while time.monotonic() < deadline:
        if process.poll() is not None:
            refuse(
                "owner_readiness",
                "owner_exited",
                f"isolated owner exited before readiness with code {process.returncode}",
            )
        if token_path.exists() or token_path.is_symlink():
            token = read_private_token(token_path)
            try:
                status, health, _ = loopback_json(
                    port, "/api/health", token, timeout=1.0
                )
            except (OSError, http.client.HTTPException):
                last_state = "health_pending"
            else:
                if (
                    status == 200
                    and isinstance(health, dict)
                    and health.get("status") == "ok"
                ):
                    return token, health
                last_state = f"health_http_{status}"
        time.sleep(0.05)
    refuse(
        "owner_readiness",
        "readiness_timeout",
        f"isolated owner not ready ({last_state})",
    )


def domain_digest(domain: bytes, payload: bytes) -> str:
    digest = hashlib.sha256()
    digest.update(ROOT_FINGERPRINT_PREFIX)
    digest.update(len(domain).to_bytes(8, "big"))
    digest.update(domain)
    digest.update(len(payload).to_bytes(8, "big"))
    digest.update(payload)
    return digest.hexdigest()


def expected_project_fingerprint(source_root: Path) -> str:
    normalized = str(source_root).replace("\\", "/").encode("utf-8")
    return f"sha256:{domain_digest(ROOT_FINGERPRINT_DOMAIN, normalized)}"


def require_mapping(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        refuse("manifest_validation", "manifest_shape", f"{label} is not an object")
    return value


def validate_owner_projection(
    manifest_response: Any,
    manifest_raw: bytes,
    stats_response: Any,
    *,
    source_root: Path,
    source: dict[str, Any],
    binary_sha256: str,
    expected_ui_sha256: str,
) -> dict[str, str]:
    response = require_mapping(manifest_response, "manifest response")
    if response.get("schema") != OWNER_RESPONSE_SCHEMA:
        refuse(
            "manifest_validation",
            "response_schema",
            "owner manifest response schema mismatch",
        )
    manifest = require_mapping(response.get("manifest"), "manifest")
    verification = require_mapping(
        response.get("verification"), "manifest verification"
    )
    if manifest.get("schema") != MANIFEST_SCHEMA:
        refuse(
            "manifest_validation", "manifest_schema", "owner manifest schema mismatch"
        )
    sealed = manifest.get("manifest_sha256")
    computed = verification.get("computed_manifest_sha256")
    if (
        not isinstance(sealed, str)
        or sealed != computed
        or HEX_SHA256.fullmatch(sealed.removeprefix("sha256:")) is None
    ):
        refuse(
            "manifest_validation",
            "manifest_self_digest",
            "manifest self digest did not verify",
        )
    if verification.get("coherence") != "COHERENT":
        refuse(
            "manifest_validation",
            "manifest_not_coherent",
            f"manifest coherence is {verification.get('coherence', 'UNKNOWN')}",
        )

    source_fact = require_mapping(manifest.get("source"), "manifest.source")
    runtime = require_mapping(manifest.get("runtime"), "manifest.runtime")
    ui = require_mapping(manifest.get("ui"), "manifest.ui")
    authorities = require_mapping(manifest.get("authorities"), "manifest.authorities")
    if (
        source_fact.get("commit") != source["commit"]
        or source_fact.get("dirty") is not False
    ):
        refuse(
            "manifest_validation",
            "source_binding",
            "manifest source does not bind clean root HEAD",
        )
    if source_fact.get("version") != source["version"]:
        refuse(
            "manifest_validation", "source_version", "manifest source version mismatch"
        )
    if runtime.get("binary_sha256") != f"sha256:{binary_sha256}":
        refuse(
            "manifest_validation",
            "runtime_binary_digest",
            "manifest binary digest mismatch",
        )
    if runtime.get("binary_version") != source["version"]:
        refuse(
            "manifest_validation", "runtime_version", "manifest binary version mismatch"
        )
    if ui.get("bundle_sha256") != f"sha256:{expected_ui_sha256}":
        refuse("manifest_validation", "served_ui_digest", "served UI digest mismatch")
    if ui.get("bundle_version") != source["version"] or ui.get("mode") != "embedded":
        refuse(
            "manifest_validation",
            "served_ui_identity",
            "served UI is not the source-version embedded release bundle",
        )
    if manifest.get("repo_id") != source_root.name:
        refuse(
            "manifest_validation",
            "served_repo_id",
            "manifest repo id differs from source root",
        )
    if manifest.get("project_root_fingerprint") != expected_project_fingerprint(
        source_root
    ):
        refuse(
            "manifest_validation",
            "served_root_fingerprint",
            "manifest project-root fingerprint differs from explicit source root",
        )
    expected_authorities = {
        "source": (source["commit"], source["version"]),
        "runtime_binary": (f"sha256:{binary_sha256}", source["commit"]),
        "ui_bundle": (f"sha256:{expected_ui_sha256}", source["version"]),
    }
    for authority_id, (digest, revision) in expected_authorities.items():
        authority = require_mapping(
            authorities.get(authority_id), f"authority {authority_id}"
        )
        if (
            authority.get("digest") != digest
            or authority.get("revision") != revision
            or authority.get("status") != "AVAILABLE"
            or authority.get("freshness") != "FRESH"
        ):
            refuse(
                "manifest_validation",
                "authority_binding",
                f"{authority_id} authority is not exact AVAILABLE/FRESH",
            )

    stats = require_mapping(stats_response, "graph stats")
    served_brain = require_mapping(
        stats.get("served_brain"), "graph stats served_brain"
    )
    served_root = served_brain.get("project_root")
    if not isinstance(served_root, str):
        refuse(
            "manifest_validation", "served_root_absent", "graph stats omits served root"
        )
    try:
        resolved_served_root = Path(served_root).resolve(strict=True)
    except OSError:
        refuse(
            "manifest_validation",
            "served_root_unreadable",
            "served root does not exist",
        )
    if resolved_served_root != source_root:
        refuse(
            "manifest_validation",
            "served_root_mismatch",
            "owner serves a different source root",
        )

    return {
        "bundle_sha256": f"sha256:{expected_ui_sha256}",
        "manifest_response_sha256": f"sha256:{sha256_bytes(manifest_raw)}",
        "manifest_sha256": sealed,
    }


def validate_owner_isolation(
    instance_response: Any,
    *,
    source_root: Path,
    prepared: dict[str, Any],
    port: int,
) -> dict[str, Any]:
    response = require_mapping(instance_response, "instance self")
    instance = require_mapping(response.get("instance"), "instance self.instance")
    graph_state = require_mapping(
        response.get("graph_state"), "instance self.graph_state"
    )
    exact_paths = {
        "workspace_root": (instance.get("workspace_root"), source_root),
        "instance_runtime_root": (instance.get("runtime_root"), prepared["runtime"]),
        "instance_graph_source": (instance.get("graph_source"), prepared["graph"]),
        "graph_runtime_root": (graph_state.get("runtime_root"), prepared["runtime"]),
        "graph_path": (graph_state.get("graph_path"), prepared["graph"]),
    }
    for label, (observed, expected) in exact_paths.items():
        if not isinstance(observed, str):
            refuse("owner_isolation", "isolation_path_absent", f"{label} is absent")
        try:
            resolved = Path(observed).resolve(strict=True)
        except OSError:
            refuse(
                "owner_isolation", "isolation_path_unreadable", f"{label} is unreadable"
            )
        if resolved != Path(expected).resolve(strict=True):
            refuse(
                "owner_isolation",
                "isolation_path_mismatch",
                f"{label} escaped isolation",
            )
    if instance.get("mode") != "read_only":
        refuse(
            "owner_isolation",
            "owner_not_read_only",
            "owner did not report read_only mode",
        )
    if instance.get("port") != port:
        refuse(
            "owner_isolation",
            "owner_port_mismatch",
            "owner registry reports another port",
        )
    if graph_state.get("workspace_root_source") != "env:M1ND_WORKSPACE_ROOT":
        refuse(
            "owner_isolation",
            "workspace_source_mismatch",
            "owner did not bind the explicit M1ND_WORKSPACE_ROOT source",
        )
    return {
        "instance_mode": "read_only",
        "read_only_observed": True,
        "runtime_root_observed_ephemeral": True,
        "source_root_observed_exact": True,
        "workspace_root_source": "env:M1ND_WORKSPACE_ROOT",
    }


class OutputDigestMonitor:
    """Hash process output without retaining it and detect an exact bearer leak."""

    def __init__(self, stream: BinaryIO, secret: bytes):
        self.stream = stream
        self.secret = secret
        self.digest = hashlib.sha256()
        self.byte_count = 0
        self.secret_detected = False
        self._tail = b""
        self._thread = threading.Thread(target=self._read, daemon=True)

    def _read(self) -> None:
        for chunk in iter(lambda: self.stream.read(64 * 1024), b""):
            self.digest.update(chunk)
            self.byte_count += len(chunk)
            combined = self._tail + chunk
            if self.secret and self.secret in combined:
                self.secret_detected = True
            keep = max(0, len(self.secret) - 1)
            self._tail = combined[-keep:] if keep else b""

    def start(self) -> None:
        self._thread.start()

    def finish(self, timeout: float = 5.0) -> None:
        self._thread.join(timeout)
        if self._thread.is_alive():
            refuse(
                "browser_gate",
                "output_monitor_stuck",
                "gate output monitor did not finish",
            )

    def receipt(self) -> dict[str, Any]:
        return {
            "byte_count": self.byte_count,
            "sha256": f"sha256:{self.digest.hexdigest()}",
            "token_detected": self.secret_detected,
        }


def resolve_command(name: str) -> Path:
    candidate = shutil.which(name, path=os.environ.get("PATH"))
    if not candidate:
        refuse(
            "dependency_preparation",
            "tool_unavailable",
            f"{name} is unavailable; nothing was installed",
        )
    try:
        resolved = Path(candidate).resolve(strict=True)
        mode = resolved.stat().st_mode
    except OSError:
        refuse(
            "dependency_preparation",
            "tool_unreadable",
            f"{name} executable could not be resolved",
        )
    if not stat.S_ISREG(mode) or not os.access(resolved, os.X_OK):
        refuse(
            "dependency_preparation",
            "tool_unsafe",
            f"resolved {name} is not an executable regular file",
        )
    return resolved


def browser_toolchain_identity() -> dict[str, Any]:
    node = resolve_command("node")
    npm = resolve_command("npm")
    npm_root = absolute_directory(npm.parent.parent, "npm package root")
    npm_package_path = absolute_regular_file(
        npm_root / "package.json", "npm package manifest"
    )
    try:
        npm_package = json.loads(npm_package_path.read_bytes())
    except (OSError, UnicodeError, json.JSONDecodeError):
        refuse(
            "dependency_preparation",
            "npm_manifest_invalid",
            "npm package manifest is unreadable",
        )
    if (
        not isinstance(npm_package, dict)
        or npm_package.get("name") != "npm"
        or not isinstance(npm_package.get("version"), str)
    ):
        refuse(
            "dependency_preparation",
            "npm_manifest_invalid",
            "resolved npm CLI is not bound to an npm package identity",
        )
    try:
        node_version_result = subprocess.run(
            [str(node), "--version"],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            env=clean_subprocess_env(),
            timeout=10,
        )
    except (OSError, subprocess.TimeoutExpired):
        refuse(
            "dependency_preparation",
            "node_version_failed",
            "resolved Node runtime could not declare its version",
        )
    node_version = node_version_result.stdout.decode("ascii", "replace").strip()
    if (
        node_version_result.returncode != 0
        or re.fullmatch(r"v\d+\.\d+\.\d+", node_version) is None
    ):
        refuse(
            "dependency_preparation",
            "node_version_invalid",
            "resolved Node runtime returned an invalid version",
        )
    npm_sha256, npm_file_count, npm_byte_count = framed_tree_identity(
        npm_root, NPM_TREE_DOMAIN, label="npm package tree"
    )
    return {
        "node": node,
        "node_version": node_version,
        "node_sha256": sha256_file(node),
        "npm": npm,
        "npm_version": npm_package["version"],
        "npm_tree_root": npm_root,
        "npm_tree_sha256": npm_sha256,
        "npm_file_count": npm_file_count,
        "npm_byte_count": npm_byte_count,
    }


def npm_environment(temporary_root: Path) -> dict[str, str]:
    environment = clean_subprocess_env()
    logs = temporary_root / "npm-logs"
    logs.mkdir(mode=0o700, exist_ok=True)
    environment.update(
        {
            "CI": "1",
            "NO_COLOR": "1",
            "NO_UPDATE_NOTIFIER": "1",
            "NPM_CONFIG_AUDIT": "false",
            "NPM_CONFIG_FUND": "false",
            "NPM_CONFIG_IGNORE_SCRIPTS": "true",
            "NPM_CONFIG_LOGS_DIR": str(logs),
            "NPM_CONFIG_OFFLINE": "true",
            "NPM_CONFIG_UPDATE_NOTIFIER": "false",
            "PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD": "1",
        }
    )
    return environment


def run_npm_ci(
    *,
    toolchain: dict[str, Any],
    workspace: Path,
    temporary_root: Path,
    timeout: float,
) -> tuple[int, dict[str, Any], bool, bool]:
    process: subprocess.Popen[bytes] | None = None
    group_id: int | None = None
    monitor: OutputDigestMonitor | None = None
    timed_out = False
    try:
        process = subprocess.Popen(
            [
                str(toolchain["node"]),
                str(toolchain["npm"]),
                "ci",
                "--offline",
                "--ignore-scripts",
                "--no-audit",
                "--no-fund",
            ],
            cwd=workspace,
            env=npm_environment(temporary_root),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        group_id = assert_new_process_group(process, "offline npm ci")
        assert process.stdout is not None
        monitor = OutputDigestMonitor(process.stdout, b"")
        monitor.start()
        try:
            return_code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            return_code = 124
    finally:
        cleaned = terminate_process_group(process, group_id)
        if monitor is not None:
            monitor.finish()
    output = monitor.receipt() if monitor is not None else {}
    return return_code, output, cleaned, timed_out


def attest_playwright_browser(
    *,
    toolchain: dict[str, Any],
    workspace: Path,
    locked: dict[str, Any],
    expected_bundle_sha256: str,
) -> dict[str, Any]:
    playwright_manifest = absolute_regular_file(
        workspace / "node_modules" / "playwright" / "package.json",
        "installed Playwright manifest",
    )
    playwright_core_manifest = absolute_regular_file(
        workspace / "node_modules" / "playwright-core" / "package.json",
        "installed playwright-core manifest",
    )
    browsers_manifest = absolute_regular_file(
        workspace / "node_modules" / "playwright-core" / "browsers.json",
        "installed Playwright browser manifest",
    )
    try:
        playwright_package = json.loads(playwright_manifest.read_bytes())
        playwright_core_package = json.loads(playwright_core_manifest.read_bytes())
        browsers = json.loads(browsers_manifest.read_bytes())["browsers"]
        chromium = next(
            row
            for row in browsers
            if isinstance(row, dict) and row.get("name") == "chromium"
        )
    except (
        OSError,
        UnicodeError,
        json.JSONDecodeError,
        KeyError,
        TypeError,
        StopIteration,
    ):
        refuse(
            "browser_identity",
            "playwright_manifest_invalid",
            "installed Playwright browser metadata is unreadable",
        )
    if (
        playwright_package.get("version") != locked["playwright_version"]
        or playwright_core_package.get("version") != locked["playwright_core_version"]
        or not isinstance(chromium.get("revision"), str)
        or not chromium["revision"].isdigit()
        or not isinstance(chromium.get("browserVersion"), str)
    ):
        refuse(
            "browser_identity",
            "playwright_version_mismatch",
            "installed Playwright/browser metadata differs from the closed lock",
        )
    query = (
        "const {chromium}=require('playwright');"
        "process.stdout.write(JSON.stringify({executablePath:chromium.executablePath()}));"
    )
    try:
        result = subprocess.run(
            [str(toolchain["node"]), "-e", query],
            cwd=workspace,
            env=npm_environment(workspace.parent),
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=15,
        )
        if result.returncode != 0 or len(result.stdout) > 64 * 1024:
            raise ValueError("browser query failed")
        executable_raw = json.loads(result.stdout)["executablePath"]
    except (
        OSError,
        subprocess.TimeoutExpired,
        UnicodeError,
        json.JSONDecodeError,
        KeyError,
        TypeError,
        ValueError,
    ):
        refuse(
            "browser_identity",
            "browser_query_failed",
            "locked Playwright could not resolve its Chromium executable",
        )
    if not isinstance(executable_raw, str):
        refuse(
            "browser_identity",
            "browser_path_invalid",
            "locked Playwright returned an invalid browser path",
        )
    executable = absolute_regular_file(
        Path(executable_raw), "Playwright Chromium executable", executable=True
    )
    root_name = f"chromium-{chromium['revision']}"
    browser_root = next(
        (parent for parent in executable.parents if parent.name == root_name), None
    )
    if browser_root is None:
        refuse(
            "browser_identity",
            "browser_revision_path_mismatch",
            "Playwright Chromium path is outside its locked revision directory",
        )
    browser_root = absolute_directory(browser_root, "Playwright Chromium bundle")
    for marker in ("INSTALLATION_COMPLETE", "DEPENDENCIES_VALIDATED"):
        absolute_regular_file(browser_root / marker, f"browser marker {marker}")
    bundle_sha256, file_count, byte_count = framed_tree_identity(
        browser_root, BROWSER_TREE_DOMAIN, label="Playwright Chromium bundle"
    )
    if bundle_sha256 != expected_bundle_sha256:
        refuse(
            "browser_identity",
            "browser_bundle_digest_mismatch",
            "Playwright Chromium bundle differs from the explicit promoted expectation",
        )
    return {
        "executable": executable,
        "executable_sha256": sha256_file(executable),
        "bundle_root": browser_root,
        "bundle_sha256": bundle_sha256,
        "file_count": file_count,
        "byte_count": byte_count,
        "playwright_version": locked["playwright_version"],
        "playwright_core_version": locked["playwright_core_version"],
        "browsers_manifest_sha256": sha256_file(browsers_manifest),
        "chromium_revision": chromium["revision"],
        "chromium_version": chromium["browserVersion"],
    }


def gate_environment(
    source_root: Path,
    prepared: dict[str, Any],
    port: int,
    expected_ui_sha256: str,
    browser_executable: Path,
) -> dict[str, str]:
    environment = clean_subprocess_env()
    environment.update(
        {
            "CI": "1",
            "M1ND_LIVE_BROWSER_EXECUTABLE": str(browser_executable),
            "M1ND_LIVE_EXPECTED_UI_BUNDLE_SHA256": expected_ui_sha256,
            "M1ND_LIVE_OWNER_TOKEN_FILE": str(prepared["runtime"] / TOKEN_FILE_NAME),
            "M1ND_LIVE_OWNER_URL": f"http://127.0.0.1:{port}",
            "M1ND_WORKSPACE_ROOT": str(source_root),
            "NO_COLOR": "1",
            "NO_UPDATE_NOTIFIER": "1",
            "NPM_CONFIG_AUDIT": "false",
            "NPM_CONFIG_FUND": "false",
            "NPM_CONFIG_OFFLINE": "true",
            "NPM_CONFIG_UPDATE_NOTIFIER": "false",
            "PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD": "1",
        }
    )
    return environment


def run_browser_gate(
    *,
    toolchain: dict[str, Any],
    workspace: Path,
    source_root: Path,
    prepared: dict[str, Any],
    port: int,
    expected_ui_sha256: str,
    browser_executable: Path,
    token: str,
    timeout: float,
) -> tuple[int, dict[str, Any], bool, bool]:
    process: subprocess.Popen[bytes] | None = None
    group_id: int | None = None
    monitor: OutputDigestMonitor | None = None
    timed_out = False
    try:
        process = subprocess.Popen(
            [
                str(toolchain["node"]),
                str(toolchain["npm"]),
                "run",
                "test:e2e:live",
            ],
            cwd=workspace,
            env=gate_environment(
                source_root,
                prepared,
                port,
                expected_ui_sha256,
                browser_executable,
            ),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        group_id = assert_new_process_group(process, "browser gate")
        assert process.stdout is not None
        monitor = OutputDigestMonitor(process.stdout, token.encode("ascii"))
        monitor.start()
        try:
            return_code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            return_code = 124
    finally:
        cleaned = terminate_process_group(process, group_id)
        if monitor is not None:
            monitor.finish()
    output = monitor.receipt() if monitor is not None else {}
    return return_code, output, cleaned, timed_out


def scrub_detail(detail: str, token: str | None) -> str:
    cleaned = detail.replace("\r", " ").replace("\n", " ")[:1000]
    return cleaned.replace(token, "[REDACTED]") if token else cleaned


def source_unchanged(source_root: Path, before: dict[str, Any]) -> bool:
    return (
        git_output(source_root, "rev-parse", "HEAD") == before["commit"]
        and git_output(source_root, "rev-parse", "HEAD^{tree}") == before["tree"]
        and not git_output(source_root, "status", "--porcelain")
    )


def execute(args: argparse.Namespace) -> dict[str, Any]:
    if os.name != "posix":
        refuse(
            "input_validation",
            "process_group_unsupported",
            "this gate requires POSIX process-group semantics",
        )
    binary = absolute_regular_file(args.binary, "--binary", executable=True)
    source_root = absolute_directory(args.source_root, "--source-root")
    output = validate_output_path(args.output)
    validate_static_topology(binary, source_root, output)
    expected_binary_sha256 = normalize_sha256(
        args.expected_binary_sha256, "--expected-binary-sha256"
    )
    expected_ui_sha256 = normalize_sha256(
        args.expected_ui_bundle_sha256, "--expected-ui-bundle-sha256"
    )
    expected_browser_sha256 = normalize_sha256(
        args.expected_browser_bundle_sha256, "--expected-browser-bundle-sha256"
    )
    if (
        not math.isfinite(args.readiness_timeout)
        or args.readiness_timeout <= 0
        or not math.isfinite(args.npm_ci_timeout)
        or args.npm_ci_timeout <= 0
        or not math.isfinite(args.gate_timeout)
        or args.gate_timeout <= 0
    ):
        refuse(
            "input_validation",
            "invalid_timeout",
            "timeouts must be finite and positive",
        )
    observed_binary_sha256 = sha256_file(binary)
    if observed_binary_sha256 != expected_binary_sha256:
        refuse(
            "binary_validation",
            "binary_digest_mismatch",
            "supplied binary digest mismatch",
        )
    source = inspect_source(source_root, expected_ui_sha256)
    toolchain = browser_toolchain_identity()

    receipt: dict[str, Any] = {
        "schema": SCHEMA,
        "captured_at_ms": int(time.time() * 1000),
        "status": "FAIL",
        "verdict": "NOT_PROVEN",
        "subject": {
            "binary_path": str(binary),
            "source_root": str(source_root),
        },
        "digests": {
            "binary_sha256": f"sha256:{observed_binary_sha256}",
            "source_commit": source["commit"],
            "source_tree": source["tree"],
            "source_ui_dist_sha256": f"sha256:{source['ui_dist_sha256']}",
            "source_ui_package_lock_sha256": f"sha256:{source['ui_package_lock_sha256']}",
        },
        "owner": {
            "bind": "127.0.0.1",
            "installed_port_1338_contacted": False,
            "installed_service_discovered": False,
            "read_only_requested": True,
            "runtime_ephemeral_requested": True,
            "registry_ephemeral_requested": True,
            "source_binding_requested": "M1ND_WORKSPACE_ROOT+cwd+served_brain",
            "process_group_created": False,
        },
        "browser_gate": {
            "command": ["node", "npm", "run", "test:e2e:live"],
            "ran": False,
            "preexisting_source_node_modules_used": False,
            "offline_locked_install": {
                "command": [
                    "node",
                    "npm",
                    "ci",
                    "--offline",
                    "--ignore-scripts",
                ],
                "ran": False,
            },
            "toolchain": {
                "node_version": toolchain["node_version"],
                "node_sha256": f"sha256:{toolchain['node_sha256']}",
                "npm_version": toolchain["npm_version"],
                "npm_tree_sha256": f"sha256:{toolchain['npm_tree_sha256']}",
                "npm_file_count": toolchain["npm_file_count"],
            },
        },
        "cleanup": {
            "browser_process_group_terminated": None,
            "owner_process_group_terminated": None,
            "ephemeral_state_removed": None,
        },
        "proof_boundary": {
            "proves_on_pass": [
                "exact supplied binary and clean source identities",
                "ephemeral UI harness dependencies installed offline from the closed SHA-512 lock",
                "explicit Playwright/Chromium revision and browser-bundle expectation",
                "isolated read-only owner on numeric loopback excluding port 1338",
                "real browser shell and owner API reads with exact embedded UI attestation",
            ],
            "does_not_use": [
                "installed M1ND service",
                "ambient port discovery or process discovery",
                "network dependency installation or source-tree node_modules",
                "mock routes, HARs, or a Playwright webServer",
            ],
        },
    }

    owner: subprocess.Popen[bytes] | None = None
    owner_group: int | None = None
    token: str | None = None
    temporary_path: Path | None = None
    try:
        with (
            tempfile.TemporaryDirectory(prefix="m1nd-g7-live-") as temporary,
            ExitStack() as owner_lifetime,
        ):
            temp_root = Path(temporary).resolve(strict=True)
            temporary_path = temp_root
            if is_within(temp_root, source_root) or is_within(source_root, temp_root):
                refuse(
                    "runtime_preparation",
                    "temp_topology",
                    "ephemeral state overlaps source root",
                )
            workspace = stage_git_ui_tree(source_root, source["commit"], temp_root)
            locked = validate_locked_ui_dependencies(workspace)
            if locked["package_lock_sha256"] != source["ui_package_lock_sha256"]:
                refuse(
                    "dependency_preparation",
                    "staged_lock_mismatch",
                    "staged package lock differs from the exact source lock",
                )
            harness_sha256, harness_file_count, harness_byte_count = (
                framed_tree_identity(
                    workspace,
                    HARNESS_TREE_DOMAIN,
                    label="ephemeral UI harness source",
                    exclude_top_level=frozenset({"node_modules"}),
                )
            )
            receipt["browser_gate"].update(
                {
                    "harness_sha256": f"sha256:{harness_sha256}",
                    "harness_file_count": harness_file_count,
                    "harness_byte_count": harness_byte_count,
                    "package_json_sha256": f"sha256:{locked['package_json_sha256']}",
                    "package_lock_sha256": f"sha256:{locked['package_lock_sha256']}",
                    "locked_dependency_count": locked["dependency_count"],
                }
            )
            receipt["browser_gate"]["offline_locked_install"]["ran"] = True
            ci_code, ci_output, ci_cleaned, ci_timed_out = run_npm_ci(
                toolchain=toolchain,
                workspace=workspace,
                temporary_root=temp_root,
                timeout=args.npm_ci_timeout,
            )
            receipt["browser_gate"]["offline_locked_install"].update(
                {
                    "exit_code": ci_code,
                    "output": ci_output,
                    "process_group_terminated": ci_cleaned,
                }
            )
            if ci_timed_out:
                refuse(
                    "dependency_preparation",
                    "npm_ci_timeout",
                    "offline npm ci exceeded its explicit timeout",
                )
            if ci_code != 0 or not ci_cleaned:
                refuse(
                    "dependency_preparation",
                    "npm_ci_failed",
                    "offline ignore-scripts npm ci failed or survived cleanup",
                )
            dependency_root = absolute_directory(
                workspace / "node_modules", "ephemeral node_modules"
            )
            dependency_sha256, dependency_files, dependency_bytes = (
                framed_tree_identity(
                    dependency_root,
                    DEPENDENCY_TREE_DOMAIN,
                    label="ephemeral locked node_modules",
                )
            )
            receipt["browser_gate"].update(
                {
                    "node_modules_sha256": f"sha256:{dependency_sha256}",
                    "node_modules_file_count": dependency_files,
                    "node_modules_byte_count": dependency_bytes,
                }
            )
            browser = attest_playwright_browser(
                toolchain=toolchain,
                workspace=workspace,
                locked=locked,
                expected_bundle_sha256=expected_browser_sha256,
            )
            receipt["browser_gate"]["browser"] = {
                "expected_bundle_sha256": f"sha256:{expected_browser_sha256}",
                "bundle_sha256": f"sha256:{browser['bundle_sha256']}",
                "bundle_file_count": browser["file_count"],
                "bundle_byte_count": browser["byte_count"],
                "executable_sha256": f"sha256:{browser['executable_sha256']}",
                "playwright_version": browser["playwright_version"],
                "playwright_core_version": browser["playwright_core_version"],
                "browsers_manifest_sha256": f"sha256:{browser['browsers_manifest_sha256']}",
                "chromium_revision": browser["chromium_revision"],
                "chromium_version": browser["chromium_version"],
            }
            prepared = prepare_ephemeral_runtime(source_root, temp_root)
            receipt["digests"]["graph_snapshot_sha256"] = (
                f"sha256:{prepared['graph_sha256']}"
            )
            if prepared["plasticity_sha256"]:
                receipt["digests"]["plasticity_state_sha256"] = (
                    f"sha256:{prepared['plasticity_sha256']}"
                )
            reservation: socket.socket | None = None
            launch_port = 0
            if args.port is not None:
                reservation, launch_port = reserve_loopback_port(args.port)
            command = owner_command(binary, prepared, launch_port)
            if reservation is not None:
                reservation.close()
            owner = subprocess.Popen(
                command,
                cwd=source_root,
                env=owner_environment(source_root, prepared, source),
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            owner_group = assert_new_process_group(owner, "isolated owner")
            receipt["owner"]["process_group_created"] = True
            # Contexts close in reverse order: the group is terminated/reaped
            # before TemporaryDirectory removes its runtime and registry roots.
            owner_lifetime.callback(terminate_process_group, owner, owner_group)
            port = wait_for_owned_endpoint(
                process=owner,
                registry=prepared["registry"],
                requested_port=args.port,
                timeout=args.readiness_timeout,
            )
            receipt["owner"]["port"] = port
            token_path = prepared["runtime"] / TOKEN_FILE_NAME
            token, health = wait_for_owner(
                owner, port, token_path, args.readiness_timeout
            )
            receipt["owner"]["health_status"] = health.get("status")

            instance_status, instance_response, _ = loopback_json(
                port, "/api/instance/self", token, timeout=args.readiness_timeout
            )
            if instance_status != 200:
                refuse(
                    "owner_isolation",
                    "instance_self_http",
                    f"instance self returned HTTP {instance_status}",
                )
            receipt["owner"].update(
                validate_owner_isolation(
                    instance_response,
                    source_root=source_root,
                    prepared=prepared,
                    port=port,
                )
            )

            status, manifest_response, manifest_raw = loopback_json(
                port, "/api/manifest", token, timeout=args.readiness_timeout
            )
            if status != 200:
                refuse(
                    "manifest_validation",
                    "manifest_http",
                    f"manifest returned HTTP {status}",
                )
            stats_status, stats, _ = loopback_json(
                port, "/api/graph/stats", token, timeout=args.readiness_timeout
            )
            if stats_status != 200:
                refuse(
                    "manifest_validation",
                    "stats_http",
                    f"graph stats returned HTTP {stats_status}",
                )
            projection = validate_owner_projection(
                manifest_response,
                manifest_raw,
                stats,
                source_root=source_root,
                source=source,
                binary_sha256=observed_binary_sha256,
                expected_ui_sha256=expected_ui_sha256,
            )
            receipt["digests"].update(projection)
            receipt["browser_gate"]["ran"] = True
            exit_code, output_receipt, browser_cleaned, gate_timed_out = (
                run_browser_gate(
                    toolchain=toolchain,
                    workspace=workspace,
                    source_root=source_root,
                    prepared=prepared,
                    port=port,
                    expected_ui_sha256=expected_ui_sha256,
                    browser_executable=browser["executable"],
                    token=token,
                    timeout=args.gate_timeout,
                )
            )
            receipt["browser_gate"].update(
                {"exit_code": exit_code, "output": output_receipt}
            )
            receipt["cleanup"]["browser_process_group_terminated"] = browser_cleaned
            if output_receipt.get("token_detected") is True:
                refuse(
                    "browser_gate",
                    "token_output_leak",
                    "browser gate emitted the owner bearer",
                )
            if gate_timed_out:
                refuse(
                    "browser_gate",
                    "gate_timeout",
                    "browser gate exceeded its explicit timeout",
                )
            if exit_code != 0:
                refuse(
                    "browser_gate",
                    "gate_failed",
                    f"G7 LIVE exited with code {exit_code}",
                )
            if not browser_cleaned:
                refuse(
                    "cleanup",
                    "browser_group_survived",
                    "browser process group survived cleanup",
                )
            final_harness_sha256, _, _ = framed_tree_identity(
                workspace,
                HARNESS_TREE_DOMAIN,
                label="ephemeral UI harness source",
                exclude_top_level=frozenset({"node_modules"}),
            )
            final_dependency_sha256, _, _ = framed_tree_identity(
                dependency_root,
                DEPENDENCY_TREE_DOMAIN,
                label="ephemeral locked node_modules",
            )
            final_browser_sha256, _, _ = framed_tree_identity(
                browser["bundle_root"],
                BROWSER_TREE_DOMAIN,
                label="Playwright Chromium bundle",
            )
            final_npm_sha256, _, _ = framed_tree_identity(
                toolchain["npm_tree_root"],
                NPM_TREE_DOMAIN,
                label="npm package tree",
            )
            if (
                final_harness_sha256 != harness_sha256
                or final_dependency_sha256 != dependency_sha256
                or final_browser_sha256 != browser["bundle_sha256"]
                or final_npm_sha256 != toolchain["npm_tree_sha256"]
                or sha256_file(toolchain["node"]) != toolchain["node_sha256"]
            ):
                refuse(
                    "input_identity",
                    "browser_input_changed",
                    "harness, dependencies, browser, Node, or npm changed during G7 LIVE",
                )
            if not source_unchanged(source_root, source):
                refuse(
                    "source_validation",
                    "source_changed",
                    "source changed while G7 LIVE ran",
                )
            receipt["status"] = "PASS"
            receipt["verdict"] = "PROVEN"
    except G7OrchestratorError as error:
        receipt["failure"] = {
            "stage": error.stage,
            "code": error.code,
            "detail": scrub_detail(error.detail, token),
        }
    except (OSError, subprocess.SubprocessError, ValueError) as error:
        receipt["failure"] = {
            "stage": "orchestrator",
            "code": "unexpected_runtime_failure",
            "detail": scrub_detail(f"{type(error).__name__}: {error}", token),
        }
    finally:
        owner_cleaned = terminate_process_group(owner, owner_group)
        receipt["cleanup"]["owner_process_group_terminated"] = owner_cleaned
        temporary_removed = temporary_path is None or not temporary_path.exists()
        receipt["cleanup"]["ephemeral_state_removed"] = temporary_removed
        if not owner_cleaned and receipt["status"] == "PASS":
            receipt["status"] = "FAIL"
            receipt["verdict"] = "NOT_PROVEN"
            receipt["failure"] = {
                "stage": "cleanup",
                "code": "owner_group_survived",
                "detail": "isolated owner process group survived cleanup",
            }
        if not temporary_removed and receipt["status"] == "PASS":
            receipt["status"] = "FAIL"
            receipt["verdict"] = "NOT_PROVEN"
            receipt["failure"] = {
                "stage": "cleanup",
                "code": "ephemeral_state_survived",
                "detail": "ephemeral runtime or registry state survived cleanup",
            }
    if token and token in json.dumps(receipt, sort_keys=True):
        refuse(
            "receipt", "token_in_receipt", "receipt serialization contained the bearer"
        )
    return receipt


def atomic_receipt(path: Path, receipt: dict[str, Any]) -> None:
    payload = (json.dumps(receipt, indent=2, sort_keys=True) + "\n").encode("utf-8")
    temporary = path.parent / f".{path.name}.tmp-{os.getpid()}"
    descriptor: int | None = None
    try:
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "wb", closefd=True) as handle:
            descriptor = None
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        directory_fd = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    finally:
        if descriptor is not None:
            os.close(descriptor)
        try:
            temporary.unlink()
        except FileNotFoundError:
            pass


def parser() -> argparse.ArgumentParser:
    argument_parser = argparse.ArgumentParser(description=__doc__)
    argument_parser.add_argument("--binary", type=Path, required=True)
    argument_parser.add_argument("--expected-binary-sha256", required=True)
    argument_parser.add_argument("--source-root", type=Path, required=True)
    argument_parser.add_argument("--expected-ui-bundle-sha256", required=True)
    argument_parser.add_argument("--expected-browser-bundle-sha256", required=True)
    argument_parser.add_argument("--port", type=int)
    argument_parser.add_argument("--readiness-timeout", type=float, default=45.0)
    argument_parser.add_argument("--npm-ci-timeout", type=float, default=180.0)
    argument_parser.add_argument("--gate-timeout", type=float, default=120.0)
    argument_parser.add_argument("--output", type=Path, required=True)
    return argument_parser


def main() -> int:
    args = parser().parse_args()
    try:
        output = validate_output_path(args.output)
        receipt = execute(args)
        atomic_receipt(output, receipt)
    except G7OrchestratorError as error:
        print(
            f"G7 LIVE orchestrator refused [{error.stage}/{error.code}]: {error.detail}",
            file=sys.stderr,
        )
        return 2
    print(f"G7 LIVE {receipt['verdict']}; receipt={output}")
    return 0 if receipt["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
