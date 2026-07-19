#!/usr/bin/env python3
"""Create and verify the exact UI tree identity consumed by release builds.

The framing intentionally mirrors ``m1nd-mcp/ui_bundle_support.rs``.  Release
jobs use this script before Cargo runs, while the binary recomputes the same
identity over the bytes materialized by rust-embed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from pathlib import Path
from typing import Any


SCHEMA = "m1nd-ui-bundle-provenance-v1"
DOMAIN = b"m1nd-ui-bundle-tree-v1\0"
PLACEHOLDER_MARKER = b"m1nd UI not built"
REQUIRED_FIELDS = {
    "schema",
    "bundle_sha256",
    "file_count",
    "node_version",
    "npm_version",
    "package_lock_sha256",
    "package_version",
    "placeholder",
    "source_commit",
}


class UiBundleError(RuntimeError):
    pass


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ui_tree_identity(root: Path) -> tuple[str, int, bool]:
    if not root.is_dir():
        raise UiBundleError(f"UI dist directory does not exist: {root}")
    files = sorted(path for path in root.rglob("*") if path.is_file())
    if not files:
        raise UiBundleError("UI dist tree is empty")
    index = root / "index.html"
    if not index.is_file():
        raise UiBundleError("UI dist tree has no index.html")

    digest = hashlib.sha256()
    digest.update(DOMAIN)
    placeholder = False
    for path in files:
        relative = path.relative_to(root).as_posix().encode("utf-8")
        payload = path.read_bytes()
        if relative == b"index.html" and PLACEHOLDER_MARKER in payload:
            placeholder = True
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        digest.update(len(payload).to_bytes(8, "big"))
        digest.update(payload)
    return digest.hexdigest(), len(files), placeholder


def package_version(path: Path) -> str:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise UiBundleError(f"unable to read UI package.json: {error}") from error
    version = value.get("version") if isinstance(value, dict) else None
    if not isinstance(version, str) or not version.strip():
        raise UiBundleError("UI package.json has no non-empty version")
    return version


def validate_commit(value: str) -> str:
    if not re.fullmatch(r"[0-9a-f]{40}", value):
        raise UiBundleError("source commit must be a full lowercase 40-character SHA-1")
    return value


def atomic_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp-{os.getpid()}")
    temporary.write_text(
        json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    os.replace(temporary, path)


def create(args: argparse.Namespace) -> str:
    commit = validate_commit(args.commit)
    version = package_version(args.package_json)
    if args.expected_version and version != args.expected_version:
        raise UiBundleError(
            f"UI package version {version!r} does not match release {args.expected_version!r}"
        )
    if not args.package_lock.is_file():
        raise UiBundleError(f"UI package lock does not exist: {args.package_lock}")
    if not args.node_version.strip() or not args.npm_version.strip():
        raise UiBundleError("Node and npm versions must be recorded")
    bundle_sha256, file_count, placeholder = ui_tree_identity(args.dist)
    if placeholder:
        raise UiBundleError("placeholder UI tree is forbidden in a release artifact")
    document = {
        "schema": SCHEMA,
        "bundle_sha256": bundle_sha256,
        "file_count": file_count,
        "node_version": args.node_version.strip(),
        "npm_version": args.npm_version.strip(),
        "package_lock_sha256": sha256_file(args.package_lock),
        "package_version": version,
        "placeholder": False,
        "source_commit": commit,
    }
    atomic_json(args.output, document)
    return bundle_sha256


def load_provenance(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise UiBundleError(f"unable to read UI provenance: {error}") from error
    if not isinstance(value, dict) or set(value) != REQUIRED_FIELDS:
        actual = sorted(value) if isinstance(value, dict) else type(value).__name__
        raise UiBundleError(
            f"UI provenance fields differ from the closed schema: {actual}"
        )
    if value.get("schema") != SCHEMA:
        raise UiBundleError(f"unexpected UI provenance schema: {value.get('schema')!r}")
    validate_commit(value.get("source_commit", ""))
    digest = value.get("bundle_sha256")
    if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise UiBundleError("UI provenance bundle digest is invalid")
    if value.get("placeholder") is not False:
        raise UiBundleError("UI provenance declares a placeholder bundle")
    if not isinstance(value.get("file_count"), int) or value["file_count"] <= 0:
        raise UiBundleError("UI provenance file count is invalid")
    for field in ("node_version", "npm_version", "package_version"):
        if not isinstance(value.get(field), str) or not value[field].strip():
            raise UiBundleError(f"UI provenance {field} is empty")
    lock_digest = value.get("package_lock_sha256")
    if not isinstance(lock_digest, str) or not re.fullmatch(r"[0-9a-f]{64}", lock_digest):
        raise UiBundleError("UI provenance package-lock digest is invalid")
    return value


def verify(args: argparse.Namespace) -> str:
    document = load_provenance(args.provenance)
    observed_sha256, file_count, placeholder = ui_tree_identity(args.dist)
    observed_version = package_version(args.package_json)
    if placeholder:
        raise UiBundleError("observed UI tree contains the placeholder marker")
    checks = {
        "bundle_sha256": observed_sha256,
        "file_count": file_count,
        "package_lock_sha256": sha256_file(args.package_lock),
        "package_version": observed_version,
    }
    for field, observed in checks.items():
        if document[field] != observed:
            raise UiBundleError(
                f"UI provenance {field} mismatch: declared={document[field]!r}, observed={observed!r}"
            )
    if args.expected_commit and document["source_commit"] != args.expected_commit:
        raise UiBundleError("UI provenance commit does not match the release commit")
    if args.expected_version and document["package_version"] != args.expected_version:
        raise UiBundleError("UI provenance version does not match the release version")
    if args.expected_sha256 and document["bundle_sha256"] != args.expected_sha256:
        raise UiBundleError("UI provenance digest does not match the workflow binding")
    return document["bundle_sha256"]


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    commands = root.add_subparsers(dest="command", required=True)

    create_parser = commands.add_parser("create")
    create_parser.add_argument("--dist", type=Path, required=True)
    create_parser.add_argument("--package-json", type=Path, required=True)
    create_parser.add_argument("--package-lock", type=Path, required=True)
    create_parser.add_argument("--commit", required=True)
    create_parser.add_argument("--expected-version")
    create_parser.add_argument("--node-version", required=True)
    create_parser.add_argument("--npm-version", required=True)
    create_parser.add_argument("--output", type=Path, required=True)
    create_parser.set_defaults(run=create)

    verify_parser = commands.add_parser("verify")
    verify_parser.add_argument("--dist", type=Path, required=True)
    verify_parser.add_argument("--package-json", type=Path, required=True)
    verify_parser.add_argument("--package-lock", type=Path, required=True)
    verify_parser.add_argument("--provenance", type=Path, required=True)
    verify_parser.add_argument("--expected-commit")
    verify_parser.add_argument("--expected-version")
    verify_parser.add_argument("--expected-sha256")
    verify_parser.set_defaults(run=verify)
    return root


def main() -> int:
    args = parser().parse_args()
    try:
        print(args.run(args))
    except (OSError, UiBundleError, ValueError) as error:
        print(f"UI bundle refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
