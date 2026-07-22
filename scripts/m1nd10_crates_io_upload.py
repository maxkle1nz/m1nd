#!/usr/bin/env python3
"""Inspect, extract, and publish exact candidate-sealed ``.crate`` bytes.

``cargo publish`` always packages the checkout again.  M1ND instead runs
``cargo package`` once before candidate freeze and uses the documented Cargo
registry Web API to upload that exact archive.  The token is read only from
``CARGO_REGISTRY_TOKEN`` and redirects are refused so credentials cannot leave
the fixed crates.io origin.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import struct
import sys
import tarfile
import time
import tomllib
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path, PurePosixPath
from typing import Any


CRATES_IO_UPLOAD_URL = "https://crates.io/api/v1/crates/new"
CRATES_IO_API_ORIGIN = "https://crates.io"
TOKEN_ENV = "CARGO_REGISTRY_TOKEN"
PUBLISHED_CRATE_ORDER = (
    "m1nd-core",
    "m1nd-control",
    "m1nd-ingest",
    "m1nd-mcp",
)
SEMVER_RE = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?")
SHA256_RE = re.compile(r"[0-9a-f]{64}")
SHA1_RE = re.compile(r"[0-9a-f]{40}")
CRATE_NAME_RE = re.compile(r"[A-Za-z0-9_-]+")
MAX_CRATE_BYTES = 10 * 1024 * 1024
MAX_UNPACKED_BYTES = 128 * 1024 * 1024
MAX_MEMBER_BYTES = 64 * 1024 * 1024
MAX_MEMBERS = 20_000
MAX_MANIFEST_BYTES = 2 * 1024 * 1024
MAX_README_BYTES = 2 * 1024 * 1024
UI_DIGEST_DOMAIN = b"m1nd-ui-bundle-tree-v1\0"
UI_PLACEHOLDER_MARKER = b"m1nd UI not built"
CRATES_IO_LOCK_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"


class CratePackageError(RuntimeError):
    pass


class RefuseRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):  # noqa: ANN001
        raise urllib.error.HTTPError(
            req.full_url,
            code,
            f"registry redirect refused: {msg}",
            headers,
            fp,
        )


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _require_string(value: Any, description: str, *, allow_empty: bool = False) -> str:
    if not isinstance(value, str) or (not allow_empty and not value.strip()):
        raise CratePackageError(f"{description} must be a non-empty string")
    return value


def _optional_string(value: Any, description: str) -> str | None:
    if value is None or value is False:
        return None
    return _require_string(value, description)


def _string_list(value: Any, description: str) -> list[str]:
    if value is None:
        return []
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise CratePackageError(f"{description} must be an array of strings")
    return list(value)


def _safe_member_name(name: str) -> PurePosixPath:
    if "\\" in name:
        raise CratePackageError(f"crate archive contains a non-POSIX path: {name!r}")
    path = PurePosixPath(name)
    if path.is_absolute() or not path.parts or any(part in {"", ".", ".."} for part in path.parts):
        raise CratePackageError(f"crate archive contains an unsafe path: {name!r}")
    return path


def _validated_archive(path: Path) -> tuple[str, dict[str, tarfile.TarInfo]]:
    if path.is_symlink() or not path.is_file():
        raise CratePackageError(f"crate artifact is not a regular file: {path}")
    size = path.stat().st_size
    if size <= 0 or size > MAX_CRATE_BYTES:
        raise CratePackageError(
            f"crate artifact size {size} is outside 1..{MAX_CRATE_BYTES} bytes: {path.name}"
        )
    try:
        with tarfile.open(path, "r:gz") as archive:
            members = archive.getmembers()
    except (OSError, tarfile.TarError) as error:
        raise CratePackageError(f"invalid .crate archive {path.name}: {error}") from error
    if not members or len(members) > MAX_MEMBERS:
        raise CratePackageError(f"crate archive member count is invalid: {len(members)}")
    by_name: dict[str, tarfile.TarInfo] = {}
    roots: set[str] = set()
    unpacked = 0
    for member in members:
        member_path = _safe_member_name(member.name)
        roots.add(member_path.parts[0])
        if member.name in by_name:
            raise CratePackageError(f"duplicate crate archive member: {member.name}")
        if not (member.isdir() or member.isfile()):
            raise CratePackageError(f"crate archive links/special files are forbidden: {member.name}")
        if member.isfile():
            if member.size < 0 or member.size > MAX_MEMBER_BYTES:
                raise CratePackageError(f"crate archive member is too large: {member.name}")
            unpacked += member.size
        by_name[member.name] = member
    if len(roots) != 1 or unpacked > MAX_UNPACKED_BYTES:
        raise CratePackageError(
            f"crate archive root/expanded size is invalid: roots={sorted(roots)}, bytes={unpacked}"
        )
    return next(iter(roots)), by_name


def _read_member(
    path: Path,
    members: dict[str, tarfile.TarInfo],
    name: str,
    *,
    limit: int = MAX_MEMBER_BYTES,
) -> bytes:
    member = members.get(name)
    if member is None or not member.isfile():
        raise CratePackageError(f"required regular crate member is missing: {name}")
    if member.size > limit:
        raise CratePackageError(f"crate member exceeds its read limit: {name}")
    try:
        with tarfile.open(path, "r:gz") as archive:
            handle = archive.extractfile(member)
            if handle is None:
                raise CratePackageError(f"unable to read crate member: {name}")
            with handle:
                payload = handle.read(limit + 1)
    except (OSError, tarfile.TarError) as error:
        raise CratePackageError(f"unable to read crate member {name}: {error}") from error
    if len(payload) > limit or len(payload) != member.size:
        raise CratePackageError(f"crate member length mismatch: {name}")
    return payload


def _safe_relative_member(root: str, relative: str, description: str) -> str:
    candidate = _safe_member_name(relative)
    if len(candidate.parts) < 1:
        raise CratePackageError(f"{description} is empty")
    return f"{root}/{candidate.as_posix()}"


def _dependency_rows(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []

    def consume(table: Any, *, kind: str, platform: str | None) -> None:
        if table is None:
            return
        if not isinstance(table, dict):
            raise CratePackageError(f"{kind} dependencies must be a table")
        for alias, raw in table.items():
            _require_string(alias, "dependency name")
            if isinstance(raw, str):
                spec: dict[str, Any] = {"version": raw}
            elif isinstance(raw, dict):
                spec = raw
            else:
                raise CratePackageError(f"dependency {alias} has an invalid specification")
            version = _require_string(spec.get("version"), f"dependency {alias} version")
            registry = spec.get("registry")
            if registry not in (None, "crates-io"):
                raise CratePackageError(
                    f"dependency {alias} uses an alternate registry; crates.io publication refused"
                )
            actual_name = _require_string(spec.get("package", alias), f"dependency {alias} package")
            artifact = spec.get("artifact")
            if isinstance(artifact, str):
                artifact_value: list[str] | None = [artifact]
            elif isinstance(artifact, list) and all(isinstance(item, str) for item in artifact):
                artifact_value = list(artifact)
            elif artifact is None:
                artifact_value = None
            else:
                raise CratePackageError(f"dependency {alias} artifact is invalid")
            row: dict[str, Any] = {
                "optional": bool(spec.get("optional", False)),
                "default_features": bool(spec.get("default-features", True)),
                "name": actual_name,
                "features": _string_list(spec.get("features"), f"dependency {alias} features"),
                "version_req": version,
                "target": platform,
                "kind": kind,
            }
            if actual_name != alias:
                row["explicit_name_in_toml"] = alias
            if artifact_value is not None:
                row["artifact"] = artifact_value
                bindep_target = spec.get("target")
                if bindep_target is not None:
                    row["bindep_target"] = _require_string(
                        bindep_target, f"dependency {alias} artifact target"
                    )
                if bool(spec.get("lib", False)):
                    row["lib"] = True
            rows.append(row)

    consume(manifest.get("dependencies"), kind="normal", platform=None)
    consume(manifest.get("build-dependencies"), kind="build", platform=None)
    consume(manifest.get("dev-dependencies"), kind="dev", platform=None)
    targets = manifest.get("target", {})
    if not isinstance(targets, dict):
        raise CratePackageError("target dependencies must be a table")
    for platform, target_tables in targets.items():
        if not isinstance(target_tables, dict):
            raise CratePackageError(f"target dependency table is invalid: {platform}")
        consume(target_tables.get("dependencies"), kind="normal", platform=platform)
        consume(target_tables.get("build-dependencies"), kind="build", platform=platform)
        consume(target_tables.get("dev-dependencies"), kind="dev", platform=platform)
    rows.sort(
        key=lambda row: (
            row["name"],
            row["kind"],
            row["target"] or "",
            row.get("explicit_name_in_toml", ""),
        )
    )
    return rows


def _workspace_lock_rows(
    path: Path,
    members: dict[str, tarfile.TarInfo],
    root: str,
    package_name: str,
) -> list[dict[str, str]]:
    try:
        lock = tomllib.loads(
            _read_member(path, members, f"{root}/Cargo.lock", limit=MAX_MANIFEST_BYTES).decode(
                "utf-8"
            )
        )
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise CratePackageError(f"invalid packaged Cargo.lock in {path.name}: {error}") from error
    packages = lock.get("package") if isinstance(lock, dict) else None
    if not isinstance(packages, list):
        raise CratePackageError("packaged Cargo.lock has no package array")
    rows: dict[str, dict[str, str]] = {}
    for package in packages:
        if not isinstance(package, dict):
            raise CratePackageError("packaged Cargo.lock contains a non-table package")
        name = package.get("name")
        if name not in PUBLISHED_CRATE_ORDER or name == package_name:
            continue
        if name in rows:
            raise CratePackageError(f"packaged Cargo.lock has duplicate internal crate {name}")
        version = _require_string(package.get("version"), f"Cargo.lock {name} version")
        source = _require_string(package.get("source"), f"Cargo.lock {name} source")
        checksum = _require_string(package.get("checksum"), f"Cargo.lock {name} checksum")
        if not SEMVER_RE.fullmatch(version):
            raise CratePackageError(f"Cargo.lock {name} version is invalid")
        if source != CRATES_IO_LOCK_SOURCE:
            raise CratePackageError(f"Cargo.lock {name} is not bound to crates.io")
        if not SHA256_RE.fullmatch(checksum):
            raise CratePackageError(f"Cargo.lock {name} checksum is invalid")
        rows[name] = {
            "name": name,
            "version": version,
            "source": source,
            "checksum": checksum,
        }
    return [rows[name] for name in PUBLISHED_CRATE_ORDER if name in rows]


def _ui_identity(
    path: Path, members: dict[str, tarfile.TarInfo], root: str
) -> tuple[str, int, bool] | None:
    prefix = f"{root}/ui-dist/"
    names = sorted(
        name for name, member in members.items() if name.startswith(prefix) and member.isfile()
    )
    if not names:
        return None
    digest = hashlib.sha256(UI_DIGEST_DOMAIN)
    placeholder = False
    for name in names:
        relative = name.removeprefix(prefix).encode("utf-8")
        if not relative or relative.startswith(b"/"):
            raise CratePackageError(f"invalid packaged UI path: {name}")
        payload = _read_member(path, members, name)
        if relative == b"index.html" and UI_PLACEHOLDER_MARKER in payload:
            placeholder = True
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        digest.update(len(payload).to_bytes(8, "big"))
        digest.update(payload)
    if f"{root}/ui-dist/index.html" not in names:
        raise CratePackageError("packaged UI has no index.html")
    return digest.hexdigest(), len(names), placeholder


def inspect_crate(path: Path) -> dict[str, Any]:
    root, members = _validated_archive(path)
    manifest_name = f"{root}/Cargo.toml"
    try:
        manifest = tomllib.loads(
            _read_member(path, members, manifest_name, limit=MAX_MANIFEST_BYTES).decode("utf-8")
        )
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise CratePackageError(f"invalid normalized Cargo.toml in {path.name}: {error}") from error
    package = manifest.get("package")
    if not isinstance(package, dict):
        raise CratePackageError("normalized Cargo.toml has no [package] table")
    name = _require_string(package.get("name"), "crate package name")
    version = _require_string(package.get("version"), "crate package version")
    if not CRATE_NAME_RE.fullmatch(name) or not SEMVER_RE.fullmatch(version):
        raise CratePackageError(f"invalid crate identity: {name}@{version}")
    expected_root = f"{name}-{version}"
    if root != expected_root or path.name != f"{expected_root}.crate":
        raise CratePackageError(
            f"crate filename/archive root mismatch: file={path.name}, root={root}, expected={expected_root}"
        )

    try:
        vcs = json.loads(
            _read_member(path, members, f"{root}/.cargo_vcs_info.json", limit=64 * 1024)
        )
    except json.JSONDecodeError as error:
        raise CratePackageError("crate VCS source identity is not valid JSON") from error
    git = vcs.get("git") if isinstance(vcs, dict) else None
    source_commit = git.get("sha1") if isinstance(git, dict) else None
    # Cargo only emits `dirty` in .cargo_vcs_info.json when the tree was dirty at
    # package time; on a clean tagged commit the field is absent. Treat absent as
    # not-dirty rather than rejecting it (the field being present-but-non-bool is
    # still an integrity failure).
    source_dirty = git.get("dirty", False) if isinstance(git, dict) else None
    if not isinstance(source_commit, str) or not SHA1_RE.fullmatch(source_commit):
        raise CratePackageError("crate VCS source commit is missing or invalid")
    if not isinstance(source_dirty, bool):
        raise CratePackageError("crate VCS dirty flag is missing or invalid")

    readme_file = _optional_string(package.get("readme"), "crate readme path")
    readme_content = None
    if readme_file is not None:
        readme_content = _read_member(
            path,
            members,
            _safe_relative_member(root, readme_file, "crate readme path"),
            limit=MAX_README_BYTES,
        ).decode("utf-8")
    license_file = _optional_string(package.get("license-file"), "crate license-file path")
    if license_file is not None:
        _read_member(
            path,
            members,
            _safe_relative_member(root, license_file, "crate license-file path"),
            limit=MAX_README_BYTES,
        )

    features_value = manifest.get("features", {})
    if not isinstance(features_value, dict):
        raise CratePackageError("crate features must be a table")
    features: dict[str, list[str]] = {}
    for feature, values in sorted(features_value.items()):
        features[_require_string(feature, "feature name")] = _string_list(
            values, f"feature {feature} values"
        )
    badges_value = manifest.get("badges", {})
    if not isinstance(badges_value, dict):
        raise CratePackageError("crate badges must be a table")
    badges: dict[str, dict[str, str]] = {}
    for badge, values in sorted(badges_value.items()):
        if not isinstance(values, dict) or not all(
            isinstance(key, str) and isinstance(value, str) for key, value in values.items()
        ):
            raise CratePackageError(f"crate badge {badge} is invalid")
        badges[badge] = dict(sorted(values.items()))

    deps = _dependency_rows(manifest)
    workspace_lock_dependencies = _workspace_lock_rows(
        path, members, root, name
    )
    metadata = {
        "name": name,
        "vers": version,
        "deps": deps,
        "features": features,
        "authors": _string_list(package.get("authors"), "crate authors"),
        "description": _optional_string(package.get("description"), "crate description"),
        "documentation": _optional_string(package.get("documentation"), "crate documentation"),
        "homepage": _optional_string(package.get("homepage"), "crate homepage"),
        "readme": readme_content,
        "readme_file": readme_file,
        "keywords": _string_list(package.get("keywords"), "crate keywords"),
        "categories": _string_list(package.get("categories"), "crate categories"),
        "license": _optional_string(package.get("license"), "crate license"),
        "license_file": license_file,
        "repository": _optional_string(package.get("repository"), "crate repository"),
        "badges": badges,
        "links": _optional_string(package.get("links"), "crate links"),
        "rust_version": _optional_string(package.get("rust-version"), "crate rust-version"),
    }
    ui = _ui_identity(path, members, root)
    ui_package_version = None
    if ui is not None:
        try:
            ui_package = json.loads(
                _read_member(path, members, f"{root}/ui-package.json", limit=256 * 1024)
            )
        except json.JSONDecodeError as error:
            raise CratePackageError("packaged UI package identity is not valid JSON") from error
        ui_package_version = (
            ui_package.get("version") if isinstance(ui_package, dict) else None
        )
        if not isinstance(ui_package_version, str) or not ui_package_version.strip():
            raise CratePackageError("packaged UI package version is missing")
    return {
        "name": name,
        "version": version,
        "source_commit": source_commit,
        "source_dirty": source_dirty,
        "sha256": sha256_file(path),
        "size_bytes": path.stat().st_size,
        "metadata": metadata,
        "workspace_lock_dependencies": workspace_lock_dependencies,
        "ui_bundle_sha256": ui[0] if ui is not None else None,
        "ui_file_count": ui[1] if ui is not None else None,
        "ui_placeholder": ui[2] if ui is not None else None,
        "ui_package_version": ui_package_version,
    }


def candidate_artifact(path: Path) -> dict[str, Any]:
    inspected = inspect_crate(path)
    name = inspected["name"]
    if name not in PUBLISHED_CRATE_ORDER:
        raise CratePackageError(f"unapproved crate package refused: {name}")
    workspace_dependencies = [
        {
            "kind": dependency["kind"],
            "name": dependency["name"],
            "version_req": dependency["version_req"],
        }
        for dependency in inspected["metadata"]["deps"]
        if dependency["name"] in PUBLISHED_CRATE_ORDER
    ]
    entry: dict[str, Any] = {
        "kind": "cargo_crate_package",
        "name": path.name,
        "package_name": name,
        "package_version": inspected["version"],
        "publish_order": PUBLISHED_CRATE_ORDER.index(name) + 1,
        "sha256": inspected["sha256"],
        "size_bytes": inspected["size_bytes"],
        "source_commit": inspected["source_commit"],
        "source_dirty": inspected["source_dirty"],
        "target": "crates.io",
        "workspace_dependencies": workspace_dependencies,
        "workspace_lock_dependencies": inspected["workspace_lock_dependencies"],
    }
    if inspected["ui_bundle_sha256"] is not None:
        entry.update(
            {
                "ui_bundle_sha256": inspected["ui_bundle_sha256"],
                "ui_file_count": inspected["ui_file_count"],
                "ui_package_version": inspected["ui_package_version"],
                "ui_placeholder": inspected["ui_placeholder"],
            }
        )
    return entry


def build_upload_body(path: Path, inspected: dict[str, Any] | None = None) -> bytes:
    inspected = inspected or inspect_crate(path)
    crate_bytes = path.read_bytes()
    metadata_bytes = json.dumps(
        inspected["metadata"],
        ensure_ascii=False,
        separators=(",", ":"),
    ).encode("utf-8")
    if len(metadata_bytes) > 2**32 - 1 or len(crate_bytes) > 2**32 - 1:
        raise CratePackageError("registry upload framing exceeds u32")
    return (
        struct.pack("<I", len(metadata_bytes))
        + metadata_bytes
        + struct.pack("<I", len(crate_bytes))
        + crate_bytes
    )


def _registry_opener() -> urllib.request.OpenerDirector:
    return urllib.request.build_opener(RefuseRedirects())


def _status_for(request: urllib.request.Request, opener: Any) -> int:
    try:
        with opener.open(request, timeout=20) as response:
            return int(response.status)
    except urllib.error.HTTPError as error:
        return error.code
    except urllib.error.URLError as error:
        raise CratePackageError(f"registry authority probe failed closed: {error.reason}") from error


def version_url(name: str, version: str) -> str:
    if not CRATE_NAME_RE.fullmatch(name) or not SEMVER_RE.fullmatch(version):
        raise CratePackageError(f"invalid crates.io identity: {name}@{version}")
    return (
        f"{CRATES_IO_API_ORIGIN}/api/v1/crates/"
        f"{urllib.parse.quote(name, safe='')}/{urllib.parse.quote(version, safe='')}"
    )


def require_version_absent(name: str, version: str, opener: Any | None = None) -> None:
    opener = opener or _registry_opener()
    request = urllib.request.Request(
        version_url(name, version),
        headers={"Accept": "application/json", "User-Agent": "m1nd-exact-crate-publisher/1"},
        method="GET",
    )
    status = _status_for(request, opener)
    if status == 404:
        return
    if status == 200:
        raise CratePackageError(f"{name}@{version} already exists; immutable publication refused")
    raise CratePackageError(
        f"{name}@{version} nonexistence is NOT_PROVEN (HTTP {status}); publication refused"
    )


def upload_exact_crate(
    path: Path,
    *,
    expected_name: str,
    expected_version: str,
    expected_sha256: str,
    token: str,
    opener: Any | None = None,
) -> dict[str, Any]:
    if not isinstance(token, str) or not token or len(token) > 512 or "\r" in token or "\n" in token:
        raise CratePackageError("CARGO_REGISTRY_TOKEN is absent or invalid")
    if not SHA256_RE.fullmatch(expected_sha256):
        raise CratePackageError("expected crate digest is not lowercase SHA-256")
    inspected = inspect_crate(path)
    if (
        inspected["name"] != expected_name
        or inspected["version"] != expected_version
        or inspected["sha256"] != expected_sha256
    ):
        raise CratePackageError("downloaded .crate identity/digest differs from candidate")
    opener = opener or _registry_opener()
    require_version_absent(expected_name, expected_version, opener)
    request = urllib.request.Request(
        CRATES_IO_UPLOAD_URL,
        data=build_upload_body(path, inspected),
        headers={
            "Accept": "application/json",
            "Authorization": token,
            "Content-Type": "application/octet-stream",
            "User-Agent": "m1nd-exact-crate-publisher/1",
        },
        method="PUT",
    )
    try:
        with opener.open(request, timeout=120) as response:
            status = int(response.status)
            payload = response.read(2 * 1024 * 1024 + 1)
    except urllib.error.HTTPError as error:
        detail = error.read(64 * 1024).decode("utf-8", errors="replace")
        raise CratePackageError(
            f"crates.io rejected {expected_name}@{expected_version} (HTTP {error.code}): {detail}"
        ) from error
    except urllib.error.URLError as error:
        raise CratePackageError(
            "crate upload outcome is indeterminate; do not retry until crates.io identity is checked"
        ) from error
    if status < 200 or status >= 300 or len(payload) > 2 * 1024 * 1024:
        raise CratePackageError(f"invalid crates.io upload response (HTTP {status})")
    try:
        response_value = json.loads(payload or b"{}")
    except json.JSONDecodeError as error:
        raise CratePackageError("crates.io upload response is not JSON") from error
    if not isinstance(response_value, dict) or response_value.get("errors"):
        raise CratePackageError(f"crates.io upload reported errors: {response_value!r}")
    return response_value


def wait_until_visible(
    name: str,
    version: str,
    *,
    attempts: int = 40,
    interval_seconds: float = 5.0,
    opener: Any | None = None,
) -> None:
    if attempts <= 0 or interval_seconds < 0:
        raise CratePackageError("visibility wait bounds are invalid")
    opener = opener or _registry_opener()
    request = urllib.request.Request(
        version_url(name, version),
        headers={"Accept": "application/json", "User-Agent": "m1nd-exact-crate-publisher/1"},
        method="GET",
    )
    for attempt in range(1, attempts + 1):
        status = _status_for(request, opener)
        if status == 200:
            return
        if status != 404:
            raise CratePackageError(
                f"{name}@{version} visibility is NOT_PROVEN (HTTP {status})"
            )
        if attempt < attempts:
            time.sleep(interval_seconds)
    raise CratePackageError(f"timed out waiting for {name}@{version} on crates.io")


def extract_exact_crate(path: Path, destination: Path) -> Path:
    root, members = _validated_archive(path)
    if destination.exists():
        raise CratePackageError(f"extraction destination already exists: {destination}")
    destination.mkdir(parents=True, mode=0o700)
    try:
        with tarfile.open(path, "r:gz") as archive:
            for name, member in sorted(members.items()):
                relative = _safe_member_name(name)
                target = destination.joinpath(*relative.parts)
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                    continue
                target.parent.mkdir(parents=True, exist_ok=True)
                handle = archive.extractfile(member)
                if handle is None:
                    raise CratePackageError(f"unable to extract crate member: {name}")
                with handle, target.open("xb") as output:
                    shutil.copyfileobj(handle, output, length=1024 * 1024)
                target.chmod(0o755 if member.mode & 0o111 else 0o644)
    except Exception:
        shutil.rmtree(destination, ignore_errors=True)
        raise
    return destination / root


def _run_inspect(args: argparse.Namespace) -> None:
    inspected = inspect_crate(args.crate)
    if args.expected_name and inspected["name"] != args.expected_name:
        raise CratePackageError("crate name differs from expected")
    if args.expected_version and inspected["version"] != args.expected_version:
        raise CratePackageError("crate version differs from expected")
    if args.expected_sha256 and inspected["sha256"] != args.expected_sha256:
        raise CratePackageError("crate digest differs from expected")
    if args.expected_commit and inspected["source_commit"] != args.expected_commit:
        raise CratePackageError("crate VCS commit differs from expected")
    print(json.dumps(inspected, sort_keys=True, separators=(",", ":")))


def _run_upload(args: argparse.Namespace) -> None:
    token = os.environ.get(TOKEN_ENV, "")
    response = upload_exact_crate(
        args.crate,
        expected_name=args.expected_name,
        expected_version=args.expected_version,
        expected_sha256=args.expected_sha256,
        token=token,
    )
    warnings = response.get("warnings", {})
    print(
        json.dumps(
            {
                "name": args.expected_name,
                "version": args.expected_version,
                "sha256": args.expected_sha256,
                "uploaded_exact_candidate_bytes": True,
                "warnings": warnings if isinstance(warnings, dict) else {},
            },
            sort_keys=True,
        )
    )


def _run_extract(args: argparse.Namespace) -> None:
    print(extract_exact_crate(args.crate, args.destination))


def _run_wait(args: argparse.Namespace) -> None:
    wait_until_visible(
        args.name,
        args.version,
        attempts=args.attempts,
        interval_seconds=args.interval_seconds,
    )
    print(f"{args.name}@{args.version} is visible on crates.io")


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    commands = root.add_subparsers(dest="command", required=True)

    inspect_parser = commands.add_parser("inspect")
    inspect_parser.add_argument("--crate", type=Path, required=True)
    inspect_parser.add_argument("--expected-name")
    inspect_parser.add_argument("--expected-version")
    inspect_parser.add_argument("--expected-sha256")
    inspect_parser.add_argument("--expected-commit")
    inspect_parser.set_defaults(run=_run_inspect)

    upload_parser = commands.add_parser("upload")
    upload_parser.add_argument("--crate", type=Path, required=True)
    upload_parser.add_argument("--expected-name", required=True)
    upload_parser.add_argument("--expected-version", required=True)
    upload_parser.add_argument("--expected-sha256", required=True)
    upload_parser.set_defaults(run=_run_upload)

    extract_parser = commands.add_parser("extract")
    extract_parser.add_argument("--crate", type=Path, required=True)
    extract_parser.add_argument("--destination", type=Path, required=True)
    extract_parser.set_defaults(run=_run_extract)

    wait_parser = commands.add_parser("wait-visible")
    wait_parser.add_argument("--name", required=True)
    wait_parser.add_argument("--version", required=True)
    wait_parser.add_argument("--attempts", type=int, default=40)
    wait_parser.add_argument("--interval-seconds", type=float, default=5.0)
    wait_parser.set_defaults(run=_run_wait)
    return root


def main() -> int:
    args = parser().parse_args()
    try:
        args.run(args)
    except (CratePackageError, OSError, json.JSONDecodeError) as error:
        print(f"exact crate publication refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
