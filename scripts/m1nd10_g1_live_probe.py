#!/usr/bin/env python3
"""Prove the live G1 owner -> h4nd manifest path without overstating G2.

The probe deliberately accepts an honest DRIFT/UNKNOWN manifest. G1 proves that
the same projection is served and consumed, that missing authority remains
visible, and that the local HTTP boundary is active. It does not prove release
provenance, cryptographic authority, same-UID isolation, or autonomy.
"""

from __future__ import annotations

import argparse
import hashlib
import http.cookiejar
import json
import stat
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any


OWNER_RESPONSE_SCHEMA = "m1nd-organism-manifest-response-v1"
MANIFEST_SCHEMA = "m1nd-organism-manifest-v1"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def read_owner_bearer_token(path: Path) -> str:
    """Read the owner transport bearer without treating it as sovereign identity."""

    if path.is_symlink() or not path.is_file():
        raise ValueError("owner token file is absent, non-regular, or a symlink")
    if stat.S_IMODE(path.stat().st_mode) & 0o077:
        raise ValueError("owner token file permissions are not private")
    token = path.read_text(encoding="utf-8").strip()
    if len(token) != 64 or any(character not in "0123456789abcdef" for character in token):
        raise ValueError("owner token file is not a canonical 32-byte hex bearer")
    return token


def read_json(
    opener: urllib.request.OpenerDirector,
    url: str,
    *,
    authorization: str | None = None,
) -> dict[str, Any]:
    headers = {"Accept": "application/json", "Cache-Control": "no-store"}
    if authorization is not None:
        headers["Authorization"] = authorization
    request = urllib.request.Request(
        url,
        headers=headers,
    )
    with opener.open(request, timeout=8) as response:
        value = json.load(response)
    if not isinstance(value, dict):
        raise ValueError(f"{url} returned non-object JSON")
    return value


def require_manifest_response(value: dict[str, Any]) -> dict[str, Any]:
    if value.get("schema") != OWNER_RESPONSE_SCHEMA:
        raise ValueError("manifest response schema mismatch")
    manifest = value.get("manifest")
    verification = value.get("verification")
    if not isinstance(manifest, dict) or not isinstance(verification, dict):
        raise ValueError("manifest response omits manifest or verification")
    if manifest.get("schema") != MANIFEST_SCHEMA:
        raise ValueError("manifest schema mismatch")
    if verification.get("computed_manifest_sha256") != manifest.get("manifest_sha256"):
        raise ValueError("owner reports a manifest self-digest mismatch")
    if verification.get("coherence") not in {"COHERENT", "DRIFT", "DEGRADED", "UNKNOWN"}:
        raise ValueError("unknown manifest coherence")
    return manifest


def stable_projection(manifest: dict[str, Any]) -> dict[str, Any]:
    """Fields that must agree across two fresh projections.

    Per-observation timestamps and the manifest self-hash are intentionally
    excluded because the h4nd proxy performs a second owner read.
    """

    authorities = manifest.get("authorities")
    stable_authorities: dict[str, Any] = {}
    if isinstance(authorities, dict):
        for authority_id, fact in authorities.items():
            if isinstance(fact, dict):
                stable_authorities[authority_id] = {
                    key: fact.get(key)
                    for key in ("revision", "digest", "freshness", "status")
                }
    return {
        key: manifest.get(key)
        for key in (
            "organism_id",
            "repo_id",
            "brain_id",
            "project_root_fingerprint",
            "source",
            "runtime",
            "graph",
            "architecture",
            "ui",
            "capabilities",
            "autonomy",
            "schemas",
            "release_provenance",
        )
    } | {"authorities": stable_authorities}


def listener_is_loopback_only(port: int) -> tuple[bool, str]:
    process = subprocess.run(
        ["lsof", "-nP", f"-iTCP:{port}", "-sTCP:LISTEN"],
        check=False,
        capture_output=True,
        text=True,
    )
    output = process.stdout.strip()
    if process.returncode != 0 or not output:
        return False, "no listening process observed"
    lines = output.splitlines()[1:]
    loopback = any(
        f"127.0.0.1:{port}" in line or f"[::1]:{port}" in line for line in lines
    )
    wildcard = any(
        token in line
        for line in lines
        for token in (f"*:{port}", f"0.0.0.0:{port}", f"[::]:{port}")
    )
    return loopback and not wildcard, output


def invalid_host_status(opener: urllib.request.OpenerDirector, url: str, port: int) -> int:
    request = urllib.request.Request(
        url,
        headers={"Host": f"invalid.example:{port}", "Accept": "application/json"},
    )
    try:
        with opener.open(request, timeout=8) as response:
            return int(response.status)
    except urllib.error.HTTPError as error:
        return int(error.code)


def port_of(url: str) -> int:
    parsed = urllib.parse.urlparse(url)
    if parsed.port is not None:
        return parsed.port
    return 443 if parsed.scheme == "https" else 80


def run_probe(
    owner_url: str,
    h4nd_url: str,
    owner_binary: Path | None,
    owner_token_file: Path | None = None,
) -> dict[str, Any]:
    plain = urllib.request.build_opener()
    owner_authorization = (
        f"Bearer {read_owner_bearer_token(owner_token_file)}"
        if owner_token_file is not None
        else None
    )
    owner_response = read_json(
        plain,
        f"{owner_url.rstrip('/')}/api/manifest",
        authorization=owner_authorization,
    )
    owner_manifest = require_manifest_response(owner_response)

    jar = http.cookiejar.CookieJar()
    h4nd = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
    bootstrap = read_json(h4nd, f"{h4nd_url.rstrip('/')}/api/security/bootstrap")
    h4nd_response = read_json(h4nd, f"{h4nd_url.rstrip('/')}/api/manifest")
    h4nd_manifest = require_manifest_response(h4nd_response)

    owner_port = port_of(owner_url)
    h4nd_port = port_of(h4nd_url)
    owner_loopback, owner_listener = listener_is_loopback_only(owner_port)
    h4nd_loopback, h4nd_listener = listener_is_loopback_only(h4nd_port)
    hostile_status = invalid_host_status(
        h4nd, f"{h4nd_url.rstrip('/')}/api/manifest", h4nd_port
    )

    runtime = owner_manifest.get("runtime")
    autonomy = owner_manifest.get("autonomy")
    if not isinstance(runtime, dict) or not isinstance(autonomy, dict):
        raise ValueError("manifest runtime/autonomy fields are not objects")
    reported_binary_digest = runtime.get("binary_sha256")
    supplied_binary_digest = sha256_file(owner_binary) if owner_binary else None

    checks = {
        "owner_manifest_schema": True,
        "owner_manifest_self_digest": True,
        "h4nd_manifest_schema": True,
        "h4nd_matches_owner_projection": stable_projection(h4nd_manifest)
        == stable_projection(owner_manifest),
        "owner_listener_loopback_only": owner_loopback,
        "h4nd_listener_loopback_only": h4nd_loopback,
        "h4nd_invalid_host_refused": hostile_status == 403,
        "h4nd_boundary_disclaims_sovereign_identity": bootstrap.get("sovereign_identity")
        is False,
        "h4nd_boundary_disclaims_same_uid_isolation": bootstrap.get("same_uid_isolation")
        is False,
        "issuance_is_frozen": autonomy.get("issuance_frozen") is True,
        "supplied_owner_binary_matches_manifest": owner_binary is None
        or supplied_binary_digest == reported_binary_digest,
    }
    passed = all(checks.values())
    return {
        "schema": "m1nd10-g1-live-proof-v1",
        "captured_at_ms": int(time.time() * 1000),
        "status": "PASS" if passed else "FAIL",
        "proof_boundary": {
            "proves": [
                "live owner serves the G1 manifest",
                "live h4nd consumes the owner projection",
                "owner and h4nd listen only on loopback",
                "h4nd rejects an invalid Host header",
                "h4nd does not label its browser cookie as sovereign identity",
                "the supplied owner binary digest matches the manifest",
                "sovereign issuance remains frozen",
            ],
            "does_not_prove": [
                "manifest coherence or release provenance",
                "cryptographic client, human, policy, quorum, or safety authority",
                "same-UID isolation",
                "protected anti-rollback storage",
                "autonomous mode activation",
            ],
        },
        "observed": {
            "owner_transport_authenticated": owner_authorization is not None,
            "owner_binary_path": str(owner_binary.resolve()) if owner_binary else None,
            "coherence": owner_response["verification"]["coherence"],
            "issue_count": len(owner_response["verification"].get("issues", [])),
            "source_commit": owner_manifest.get("source", {}).get("commit"),
            "binary_sha256": reported_binary_digest,
            "ui_bundle_sha256": owner_manifest.get("ui", {}).get("bundle_sha256"),
            "active_mode": autonomy.get("active_mode"),
            "issuance_frozen": autonomy.get("issuance_frozen"),
            "hostile_host_status": hostile_status,
            "owner_listener": owner_listener,
            "h4nd_listener": h4nd_listener,
        },
        "checks": checks,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--owner-url", default="http://127.0.0.1:1338")
    parser.add_argument("--h4nd-url", default="http://127.0.0.1:3000")
    parser.add_argument("--owner-binary", type=Path)
    parser.add_argument("--owner-token-file", type=Path)
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        result = run_probe(
            args.owner_url,
            args.h4nd_url,
            args.owner_binary,
            args.owner_token_file,
        )
    except Exception as error:  # noqa: BLE001 - proof must serialize every failure
        result = {
            "schema": "m1nd10-g1-live-proof-v1",
            "captured_at_ms": int(time.time() * 1000),
            "status": "FAIL",
            "error": f"{type(error).__name__}: {error}",
        }
    rendered = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    sys.stdout.write(rendered)
    return 0 if result.get("status") == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
