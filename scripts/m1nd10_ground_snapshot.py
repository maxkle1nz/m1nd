#!/usr/bin/env python3
"""Verify the immutable G0 receipt for the M1nd 10 programme.

The receipt deliberately separates two kinds of facts:

* ``frozen_inputs`` are ratified documents and golden fixtures.  A byte change
  is a gate failure and requires a new, explicitly reviewed receipt.
* ``observations`` describe the source/runtime/UI/h4nd state seen when G0 was
  captured.  Later implementation is expected to advance some of them, so a
  mismatch is reported as drift instead of rewriting history.

The verifier never updates the receipt.  Its own digest omits only the
``receipt_sha256`` field and uses deterministic JSON (UTF-8, sorted keys, no
insignificant whitespace).  This gives G0 a hermetic, reviewable boundary
without pretending that the baseline is the current release candidate.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys
from dataclasses import dataclass
from typing import Any


SCHEMA = "m1nd-ground-snapshot-receipt-v1"
DEFAULT_RECEIPT = "docs/proofs/m1nd10-ground-snapshot-v1.json"
SHA256_RE = re.compile(r"^sha256:[0-9a-f]{64}$")


class ReceiptError(ValueError):
    """An immutable receipt invariant failed."""


@dataclass(frozen=True)
class VerificationReport:
    frozen_inputs_checked: int
    observations: tuple[dict[str, str], ...]

    def as_dict(self) -> dict[str, Any]:
        return {
            "schema": "m1nd-ground-snapshot-verification-v1",
            "status": "PASS",
            "frozen_inputs_checked": self.frozen_inputs_checked,
            "observations": list(self.observations),
        }


def canonical_json(value: Any) -> bytes:
    """Return the receipt's deterministic JSON encoding."""

    try:
        encoded = json.dumps(
            value,
            ensure_ascii=False,
            allow_nan=False,
            sort_keys=True,
            separators=(",", ":"),
        )
    except (TypeError, ValueError) as exc:
        raise ReceiptError(f"value is not canonically serializable: {exc}") from exc
    return encoded.encode("utf-8")


def sha256_bytes(data: bytes) -> str:
    return f"sha256:{hashlib.sha256(data).hexdigest()}"


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def receipt_digest(receipt: dict[str, Any]) -> str:
    core = dict(receipt)
    core.pop("receipt_sha256", None)
    return sha256_bytes(canonical_json(core))


def _safe_repo_path(repo_root: pathlib.Path, raw: str) -> pathlib.Path:
    relative = pathlib.PurePosixPath(raw)
    if relative.is_absolute() or not raw or ".." in relative.parts:
        raise ReceiptError(f"frozen input must be a safe repo-relative path: {raw!r}")
    resolved = (repo_root / pathlib.Path(*relative.parts)).resolve()
    try:
        resolved.relative_to(repo_root.resolve())
    except ValueError as exc:
        raise ReceiptError(f"frozen input escapes repository: {raw!r}") from exc
    return resolved


def _git(repo_root: pathlib.Path, *args: str) -> str | None:
    try:
        return subprocess.check_output(
            ["git", *args], cwd=repo_root, text=True, stderr=subprocess.DEVNULL
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def _tree_digest(root: pathlib.Path) -> str | None:
    if not root.is_dir():
        return None
    rows: list[dict[str, str]] = []
    for path in sorted(p for p in root.rglob("*") if p.is_file()):
        rows.append(
            {
                "path": path.relative_to(root).as_posix(),
                "sha256": sha256_file(path),
            }
        )
    return sha256_bytes(canonical_json(rows))


def _validate_shape(receipt: dict[str, Any]) -> None:
    if receipt.get("schema") != SCHEMA:
        raise ReceiptError(f"schema must be {SCHEMA!r}")
    declared = receipt.get("receipt_sha256")
    if not isinstance(declared, str) or not SHA256_RE.fullmatch(declared):
        raise ReceiptError("receipt_sha256 is absent or malformed")
    actual = receipt_digest(receipt)
    if declared != actual:
        raise ReceiptError(f"receipt self-digest mismatch: expected {declared}, got {actual}")

    frozen = receipt.get("frozen_inputs")
    if not isinstance(frozen, list) or not frozen:
        raise ReceiptError("frozen_inputs must be a non-empty list")
    paths: set[str] = set()
    for entry in frozen:
        if not isinstance(entry, dict):
            raise ReceiptError("each frozen input must be an object")
        path = entry.get("path")
        digest = entry.get("sha256")
        if not isinstance(path, str) or path in paths:
            raise ReceiptError(f"duplicate or invalid frozen input path: {path!r}")
        if not isinstance(digest, str) or not SHA256_RE.fullmatch(digest):
            raise ReceiptError(f"invalid SHA-256 for frozen input {path!r}")
        paths.add(path)


def verify_receipt(receipt_path: pathlib.Path, repo_root: pathlib.Path) -> VerificationReport:
    try:
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ReceiptError(f"cannot read receipt {receipt_path}: {exc}") from exc
    if not isinstance(receipt, dict):
        raise ReceiptError("receipt root must be an object")
    _validate_shape(receipt)

    checked = 0
    for entry in receipt["frozen_inputs"]:
        path = _safe_repo_path(repo_root, entry["path"])
        if not path.is_file():
            raise ReceiptError(f"frozen input is missing: {entry['path']}")
        actual = sha256_file(path)
        if actual != entry["sha256"]:
            raise ReceiptError(
                f"frozen input drift: {entry['path']} expected {entry['sha256']}, got {actual}"
            )
        checked += 1

    observations: list[dict[str, str]] = []
    baseline = receipt.get("observations", {})
    source = baseline.get("source", {}) if isinstance(baseline, dict) else {}
    captured_head = source.get("head") if isinstance(source, dict) else None
    current_head = _git(repo_root, "rev-parse", "HEAD")
    if captured_head and current_head:
        status = "MATCH" if captured_head == current_head else "ADVANCED"
        observations.append(
            {"fact": "source_head", "status": status, "captured": captured_head, "current": current_head}
        )
        if _git(repo_root, "cat-file", "-e", f"{captured_head}^{{commit}}") is None:
            raise ReceiptError(f"captured source commit is no longer resolvable: {captured_head}")

    ui = baseline.get("ui", {}) if isinstance(baseline, dict) else {}
    captured_ui = ui.get("bundle_tree_sha256") if isinstance(ui, dict) else None
    if isinstance(captured_ui, str):
        current_ui = _tree_digest(repo_root / "m1nd-ui" / "dist")
        observations.append(
            {
                "fact": "ui_bundle",
                "status": "MATCH" if current_ui == captured_ui else "DRIFT",
                "captured": captured_ui,
                "current": current_ui or "UNKNOWN",
            }
        )

    return VerificationReport(checked, tuple(observations))


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Verify the M1nd 10 immutable G0 receipt")
    parser.add_argument("--receipt", default=DEFAULT_RECEIPT)
    parser.add_argument("--repo", default=".")
    parser.add_argument("--json", action="store_true", dest="as_json")
    args = parser.parse_args(argv)

    repo_root = pathlib.Path(args.repo).resolve()
    receipt_path = pathlib.Path(args.receipt)
    if not receipt_path.is_absolute():
        receipt_path = repo_root / receipt_path
    try:
        report = verify_receipt(receipt_path, repo_root)
    except ReceiptError as exc:
        payload = {
            "schema": "m1nd-ground-snapshot-verification-v1",
            "status": "FAIL",
            "error": str(exc),
        }
        if args.as_json:
            print(json.dumps(payload, ensure_ascii=False, sort_keys=True))
        else:
            print(f"G0 ground snapshot: FAIL — {exc}", file=sys.stderr)
        return 1

    if args.as_json:
        print(json.dumps(report.as_dict(), ensure_ascii=False, sort_keys=True))
    else:
        print(f"G0 ground snapshot: PASS — {report.frozen_inputs_checked} frozen inputs verified")
        for observation in report.observations:
            print(f"  {observation['fact']}: {observation['status']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
