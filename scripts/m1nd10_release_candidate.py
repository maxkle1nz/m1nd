#!/usr/bin/env python3
"""Build and verify the immutable M1ND release-candidate control plane.

The script never builds or publishes. It binds each updater-facing raw runtime
to the matching archived runtime, execution receipts, and SBOM in one
content-addressed candidate. Downstream release jobs may promote only those
bytes.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tarfile
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

import m1nd10_release_contract as canonical_release  # noqa: E402
import m1nd10_crates_io_upload as crate_package  # noqa: E402
import m1nd10_ui_bundle as ui_bundle  # noqa: E402


SCHEMA = "m1nd-release-candidate-v1"
GATE_SCHEMA = "m1nd-release-gate-receipt-v1"
ROLLBACK_SCHEMA = "m1nd-release-rollback-manifest-v1"
ARCHIVE_RE = re.compile(r"^m1nd-mcp-(?P<target>[a-z0-9_-]+)\.(?:tar\.gz|zip)$")
RAW_BINARY_RE = re.compile(
    r"^m1nd-mcp-(?P<target>[a-z0-9_-]+)(?P<windows_suffix>\.exe)?$"
)
SMOKE_RECEIPT_RE = re.compile(
    r"^GATE-ARTIFACT-SMOKE-(?P<target>[a-z0-9_-]+)\.json$"
)
VERIFIED_UPDATE_RECEIPT_RE = re.compile(
    r"^GATE-VERIFIED-UPDATE-SMOKE-(?P<target>[a-z0-9_-]+)\.json$"
)
SBOM_NAME = "m1nd-mcp.spdx.json"
UI_BUNDLE_PROVENANCE_NAME = "UI-BUNDLE-PROVENANCE.json"
NPM_PACKAGE_NAME = "@maxkle1nz/m1nd"
NPM_REGISTRY = "https://registry.npmjs.org"
CONTROL_NAMES = {"CANDIDATE.json", "GATE-RECEIPT.json", "ROLLBACK.json", "SHA256SUMS"}
CONTROL_NAMES.update(
    {
        "RELEASE-COMPATIBILITY.json",
        "M1ND10-ROLLBACK.json",
        "CANONICAL-OPERATIONAL-DIGESTS.json",
        "M1ND10-EVIDENCE-SET.json",
        "M1ND10-CANONICAL-VECTORS.json",
    }
)
CANONICAL_COMPATIBILITY_SCHEMA = "m1nd-release-compatibility-manifest-v1"
CANONICAL_ROLLBACK_SCHEMA = "m1nd-release-rollback-plan-v1"
CANONICAL_OPERATIONAL_DIGESTS_SCHEMA = "m1nd-release-operational-digests-input-v1"
CANONICAL_CORE_INPUT_SCHEMA = "m1nd-release-candidate-core-input-v1"
G8_REQUIRED_ARTIFACT_KEYS = {
    "capability_matrix",
    "g8_adr",
    "host_first_minute_benchmark",
    "tool_catalog_parity",
}
VERIFIED_UPDATE_PROOFS = {
    "atomic_state_journal",
    "backup_digest_matched_pre_update",
    "exact_candidate_installed",
    "exact_pre_update_bytes_restored",
    "executable_after_rollback",
    "executable_after_update",
    "expected_commit_verified",
    "expected_version_verified",
    "idempotent_rollback",
    "live_installation_untouched",
    "no_effects_on_drift_refusal",
    "rollback_crash_recovery",
    "state_bound_backup_digest",
    "state_bound_candidate_digest",
    "target_digest_fence",
    "verified_candidate_identity",
    "verified_candidate_signature",
}


class CandidateError(ValueError):
    pass


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_stream(handle: Any) -> tuple[str, int]:
    digest = hashlib.sha256()
    size = 0
    for chunk in iter(lambda: handle.read(1024 * 1024), b""):
        digest.update(chunk)
        size += len(chunk)
    return digest.hexdigest(), size


def load_json(path: Path, description: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise CandidateError(f"invalid {description} {path.name}: {error}") from error
    if not isinstance(value, dict):
        raise CandidateError(f"{description} must be a JSON object: {path.name}")
    return value


def require_sha256(value: Any, description: str) -> str:
    if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value):
        raise CandidateError(f"{description} is not a lowercase SHA-256 digest")
    return value


def raw_asset_name(target: str) -> str:
    suffix = ".exe" if target.startswith("windows-") else ""
    return f"m1nd-mcp-{target}{suffix}"


def expected_archive_member(target: str) -> str:
    return "m1nd-mcp.exe" if target.startswith("windows-") else "m1nd-mcp"


def archived_runtime_digest(path: Path, target: str) -> tuple[str, str, int]:
    expected = expected_archive_member(target)
    if path.name.endswith(".tar.gz"):
        try:
            with tarfile.open(path, "r:gz") as archive:
                members = archive.getmembers()
                if len(members) != 1 or not members[0].isfile():
                    raise CandidateError(
                        f"archive {path.name} must contain exactly one regular runtime file"
                    )
                member = members[0]
                if member.name != expected:
                    raise CandidateError(
                        f"archive {path.name} member {member.name!r} != {expected!r}"
                    )
                handle = archive.extractfile(member)
                if handle is None:
                    raise CandidateError(f"cannot read runtime member from {path.name}")
                with handle:
                    digest, size = sha256_stream(handle)
                return member.name, digest, size
        except (tarfile.TarError, OSError) as error:
            raise CandidateError(f"invalid runtime archive {path.name}: {error}") from error
    if path.name.endswith(".zip"):
        try:
            with zipfile.ZipFile(path, "r") as archive:
                members = archive.infolist()
                if len(members) != 1 or members[0].is_dir():
                    raise CandidateError(
                        f"archive {path.name} must contain exactly one regular runtime file"
                    )
                member = members[0]
                if member.filename != expected:
                    raise CandidateError(
                        f"archive {path.name} member {member.filename!r} != {expected!r}"
                    )
                with archive.open(member, "r") as handle:
                    digest, size = sha256_stream(handle)
                return member.filename, digest, size
        except (zipfile.BadZipFile, OSError) as error:
            raise CandidateError(f"invalid runtime archive {path.name}: {error}") from error
    raise CandidateError(f"unsupported runtime archive: {path.name}")


def inspect_npm_package(path: Path) -> tuple[str, str]:
    try:
        with tarfile.open(path, "r:gz") as archive:
            package_json_members = []
            for member in archive.getmembers():
                member_path = PurePosixPath(member.name)
                if member_path.is_absolute() or ".." in member_path.parts:
                    raise CandidateError(
                        f"npm package contains an unsafe member path: {member.name!r}"
                    )
                if member.issym() or member.islnk():
                    raise CandidateError(
                        f"npm package contains a link member: {member.name!r}"
                    )
                if member.name == "package/package.json" and member.isfile():
                    package_json_members.append(member)
            if len(package_json_members) != 1:
                raise CandidateError(
                    "npm package must contain exactly one regular package/package.json"
                )
            handle = archive.extractfile(package_json_members[0])
            if handle is None:
                raise CandidateError("npm package package.json cannot be read")
            with handle:
                package = json.load(handle)
    except (OSError, tarfile.TarError, json.JSONDecodeError) as error:
        raise CandidateError(f"invalid npm package {path.name}: {error}") from error
    if not isinstance(package, dict) or package.get("name") != NPM_PACKAGE_NAME:
        raise CandidateError(
            f"npm package identity is not the canonical {NPM_PACKAGE_NAME}"
        )
    publish_config = package.get("publishConfig")
    if publish_config is not None:
        if not isinstance(publish_config, dict):
            raise CandidateError("npm package publishConfig must be an object")
        scoped_registries = sorted(
            key for key in publish_config if key.lower().endswith(":registry")
        )
        if scoped_registries:
            raise CandidateError(
                "npm package publishConfig cannot contain a scoped registry redirect"
            )
        allowed_publish_config = {"access", "registry"}
        unsupported = sorted(set(publish_config) - allowed_publish_config)
        if unsupported:
            raise CandidateError(
                f"npm package publishConfig contains unsupported keys: {unsupported}"
            )
        if publish_config.get("registry", NPM_REGISTRY) != NPM_REGISTRY:
            raise CandidateError(
                f"npm package publishConfig.registry must be exactly {NPM_REGISTRY}"
            )
        if publish_config.get("access", "public") != "public":
            raise CandidateError("npm package publishConfig.access must be exactly public")
    version = package.get("version")
    if not isinstance(version, str) or not re.fullmatch(
        r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?", version
    ):
        raise CandidateError("npm package contains an invalid version")
    return package["name"], version


def atomic_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp-{os.getpid()}")
    temporary.write_bytes(canonical_json(value))
    os.replace(temporary, path)


def atomic_canonical_json(path: Path, value: Any) -> None:
    """Write the Rust-compatible canonical form (UTF-8, no trailing newline)."""

    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp-{os.getpid()}")
    temporary.write_bytes(canonical_release.canonical_json(value))
    os.replace(temporary, path)


def collect_artifacts(
    directory: Path, *, allow_derived_controls: bool = False
) -> list[dict[str, Any]]:
    if not directory.is_dir():
        raise CandidateError(f"artifact directory does not exist: {directory}")

    artifacts: list[dict[str, Any]] = []
    for path in sorted(directory.iterdir()):
        if path.is_symlink():
            raise CandidateError(f"symlink artifact refused: {path.name}")
        if not path.is_file():
            continue
        archive_match = ARCHIVE_RE.fullmatch(path.name)
        raw_match = RAW_BINARY_RE.fullmatch(path.name)
        if archive_match:
            kind = "runtime_archive"
            target = archive_match.group("target")
        elif raw_match:
            kind = "runtime_binary"
            target = raw_match.group("target")
            if path.name != raw_asset_name(target):
                raise CandidateError(
                    f"raw runtime asset has the wrong platform suffix: {path.name}"
                )
        elif smoke_match := SMOKE_RECEIPT_RE.fullmatch(path.name):
            receipt = load_json(path, "artifact smoke receipt")
            target = smoke_match.group("target")
            if (
                receipt.get("schema") != "m1nd-release-artifact-smoke-v1"
                or receipt.get("target") != target
                or receipt.get("verdict") != "PASS"
            ):
                raise CandidateError(f"invalid or non-PASS smoke receipt: {path.name}")
            binary_sha = require_sha256(
                receipt.get("binary", {}).get("sha256"),
                f"artifact smoke receipt {path.name} binary digest",
            )
            kind = "artifact_smoke_receipt"
        elif path.name == SBOM_NAME:
            kind = "sbom_spdx_json"
            target = "all-runtime-artifacts"
        elif path.name == UI_BUNDLE_PROVENANCE_NAME:
            try:
                provenance = ui_bundle.load_provenance(path)
            except ui_bundle.UiBundleError as error:
                raise CandidateError(f"invalid UI bundle provenance: {error}") from error
            kind = "ui_bundle_provenance"
            target = "all-runtime-artifacts"
        elif path.name.endswith(".tgz"):
            npm_name, npm_version = inspect_npm_package(path)
            kind = "npm_package_tarball"
            target = "npm"
        elif path.name.endswith(".crate"):
            try:
                artifacts.append(crate_package.candidate_artifact(path))
            except crate_package.CratePackageError as error:
                raise CandidateError(f"invalid Cargo package {path.name}: {error}") from error
            continue
        elif allow_derived_controls and path.name in CONTROL_NAMES:
            continue
        elif allow_derived_controls and path.name.endswith(".sigstore.json"):
            subject_name = path.name.removesuffix(".sigstore.json")
            if subject_name in {candidate.name for candidate in directory.iterdir()}:
                continue
            raise CandidateError(
                f"signature bundle has no release subject: {path.name}"
            )
        else:
            raise CandidateError(f"unrecognized release artifact refused: {path.name}")
        if path.stat().st_size <= 0:
            raise CandidateError(f"empty artifact refused: {path.name}")
        entry = {
            "kind": kind,
            "name": path.name,
            "sha256": sha256_file(path),
            "size_bytes": path.stat().st_size,
            "target": target,
        }
        if kind == "artifact_smoke_receipt":
            entry["runtime_sha256"] = binary_sha
        elif kind == "ui_bundle_provenance":
            entry["ui_bundle_sha256"] = provenance["bundle_sha256"]
            entry["file_count"] = provenance["file_count"]
            entry["source_commit"] = provenance["source_commit"]
            entry["package_version"] = provenance["package_version"]
            entry["placeholder"] = provenance["placeholder"]
        elif kind == "npm_package_tarball":
            entry["package_name"] = npm_name
            entry["package_version"] = npm_version
        artifacts.append(entry)
    return artifacts


def bind_crate_packages(
    *,
    artifacts: list[dict[str, Any]],
    release_version: str,
    commit: str,
    ui_provenance: dict[str, Any],
) -> list[dict[str, Any]]:
    entries = [entry for entry in artifacts if entry["kind"] == "cargo_crate_package"]
    by_name: dict[str, dict[str, Any]] = {}
    for entry in entries:
        package_name = entry.get("package_name")
        if not isinstance(package_name, str) or package_name in by_name:
            raise CandidateError(f"duplicate or invalid Cargo package identity: {package_name!r}")
        by_name[package_name] = entry
    expected = set(crate_package.PUBLISHED_CRATE_ORDER)
    if set(by_name) != expected:
        raise CandidateError(
            "Cargo package set mismatch: "
            f"expected={list(crate_package.PUBLISHED_CRATE_ORDER)}, actual={sorted(by_name)}"
        )

    ordered = [by_name[name] for name in crate_package.PUBLISHED_CRATE_ORDER]
    for position, entry in enumerate(ordered, start=1):
        name = entry["package_name"]
        if entry.get("publish_order") != position:
            raise CandidateError(f"Cargo package publish order drifted for {name}")
        if entry.get("source_commit") != commit:
            raise CandidateError(f"Cargo package {name} is not bound to candidate commit")
        if not isinstance(entry.get("source_dirty"), bool):
            raise CandidateError(f"Cargo package {name} has no honest VCS dirty binding")
        if name in {"m1nd-core", "m1nd-ingest", "m1nd-mcp"} and entry.get(
            "package_version"
        ) != release_version:
            raise CandidateError(f"Cargo package {name} version is not bound to release")
        dependencies = entry.get("workspace_dependencies")
        if not isinstance(dependencies, list):
            raise CandidateError(f"Cargo package {name} dependency binding is invalid")
        earlier = set(crate_package.PUBLISHED_CRATE_ORDER[: position - 1])
        for dependency in dependencies:
            if not isinstance(dependency, dict):
                raise CandidateError(
                    f"Cargo package {name} dependency binding is invalid: {dependency!r}"
                )
            if dependency.get("kind") == "dev":
                continue
            if dependency.get("name") not in earlier:
                raise CandidateError(
                    f"Cargo package {name} dependency is not publishable earlier: {dependency!r}"
                )

    expected_edges = {
        "m1nd-core": set(),
        "m1nd-control": set(),
        "m1nd-ingest": {"m1nd-core"},
        "m1nd-mcp": {"m1nd-control", "m1nd-core", "m1nd-ingest"},
    }
    for entry in ordered:
        observed_edges = {
            dependency["name"]
            for dependency in entry["workspace_dependencies"]
            if dependency.get("kind") != "dev"
        }
        if observed_edges != expected_edges[entry["package_name"]]:
            raise CandidateError(
                f"Cargo package dependency graph drifted for {entry['package_name']}: "
                f"expected={sorted(expected_edges[entry['package_name']])}, "
                f"actual={sorted(observed_edges)}"
            )

        lock_dependencies = entry.get("workspace_lock_dependencies")
        if not isinstance(lock_dependencies, list):
            raise CandidateError(
                f"Cargo package lock binding is invalid for {entry['package_name']}"
            )
        lock_by_name: dict[str, dict[str, Any]] = {}
        for dependency in lock_dependencies:
            dependency_name = dependency.get("name") if isinstance(dependency, dict) else None
            if not isinstance(dependency_name, str) or dependency_name in lock_by_name:
                raise CandidateError(
                    f"Cargo package lock binding is duplicate/invalid for {entry['package_name']}"
                )
            lock_by_name[dependency_name] = dependency
        declared_internal = {
            dependency["name"] for dependency in entry["workspace_dependencies"]
        }
        if set(lock_by_name) != declared_internal:
            raise CandidateError(
                f"Cargo package lock dependency set drifted for {entry['package_name']}"
            )
        for dependency_name, locked in lock_by_name.items():
            dependency_package = by_name[dependency_name]
            if (
                locked.get("version") != dependency_package.get("package_version")
                or locked.get("checksum") != dependency_package.get("sha256")
                or locked.get("source") != crate_package.CRATES_IO_LOCK_SOURCE
            ):
                raise CandidateError(
                    f"Cargo package lock bytes are not candidate-bound for "
                    f"{entry['package_name']} -> {dependency_name}"
                )

    mcp = by_name["m1nd-mcp"]
    if (
        mcp.get("ui_bundle_sha256") != ui_provenance["ui_bundle_sha256"]
        or mcp.get("ui_file_count") != ui_provenance["file_count"]
        or mcp.get("ui_package_version") != ui_provenance["package_version"]
        or mcp.get("ui_placeholder") is not False
    ):
        raise CandidateError(
            "m1nd-mcp Cargo package does not contain the exact sealed UI artifact"
        )
    return ordered


def unique_target_map(
    artifacts: list[dict[str, Any]], kind: str
) -> dict[str, dict[str, Any]]:
    result: dict[str, dict[str, Any]] = {}
    for entry in artifacts:
        if entry["kind"] != kind:
            continue
        target = entry["target"]
        if target in result:
            raise CandidateError(f"duplicate {kind} for target {target}")
        result[target] = entry
    return result


def bind_runtime_artifacts(
    *,
    directory: Path,
    artifacts: list[dict[str, Any]],
    expected_targets: list[str],
    version: str,
    commit: str,
    ui_bundle_sha256: str,
) -> list[dict[str, Any]]:
    kinds = {
        kind: unique_target_map(artifacts, kind)
        for kind in (
            "runtime_archive",
            "runtime_binary",
            "artifact_smoke_receipt",
        )
    }
    expected = sorted(set(expected_targets))
    for kind, by_target in kinds.items():
        actual = sorted(by_target)
        if actual != expected:
            raise CandidateError(
                f"{kind} target set mismatch: expected={expected}, actual={actual}"
            )

    bindings: list[dict[str, Any]] = []
    for target in expected:
        archive = kinds["runtime_archive"][target]
        raw = kinds["runtime_binary"][target]
        artifact_smoke = kinds["artifact_smoke_receipt"][target]
        member, member_sha, member_size = archived_runtime_digest(
            directory / archive["name"], target
        )
        if member_sha != raw["sha256"] or member_size != raw["size_bytes"]:
            raise CandidateError(
                f"archive/raw runtime mismatch for {target}: "
                f"archive={member_sha}/{member_size}, raw={raw['sha256']}/{raw['size_bytes']}"
            )
        if artifact_smoke["runtime_sha256"] != raw["sha256"]:
            raise CandidateError(
                f"artifact smoke did not execute the archived/raw bytes for {target}"
            )

        artifact_receipt = load_json(
            directory / artifact_smoke["name"], "artifact smoke receipt"
        )
        expected_identity = artifact_receipt.get("expected", {})
        artifact_version = artifact_receipt.get("binary", {}).get("version_output", "")
        if expected_identity != {"commit": commit, "version": version}:
            raise CandidateError(
                f"artifact smoke identity does not match candidate for {target}"
            )
        if version not in artifact_version or commit[:7] not in artifact_version:
            raise CandidateError(
                f"artifact smoke version output is not source-bound for {target}"
            )
        smoke_ui = artifact_receipt.get("ui_bundle")
        if not isinstance(smoke_ui, dict) or smoke_ui != {
            "freshness": "FRESH",
            "mode": "embedded",
            "sha256": ui_bundle_sha256,
            "status": "AVAILABLE",
        }:
            raise CandidateError(
                f"artifact smoke UI identity does not match rebuilt bundle for {target}"
            )

        bindings.append(
            {
                "archive": archive["name"],
                "archive_member": member,
                "artifact_smoke_receipt": artifact_smoke["name"],
                "raw_binary": raw["name"],
                "runtime_sha256": raw["sha256"],
                "size_bytes": raw["size_bytes"],
                "target": target,
            }
        )
    return bindings


def candidate_seed(
    *,
    version: str,
    commit: str,
    source_ref: str,
    artifacts: list[dict[str, Any]],
    runtime_bindings: list[dict[str, Any]],
) -> dict[str, Any]:
    return {
        "artifacts": artifacts,
        "commit": commit,
        "runtime_bindings": runtime_bindings,
        "source_ref": source_ref,
        "version": version,
    }


def validate_identity(version: str, commit: str, source_ref: str) -> None:
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?", version):
        raise CandidateError(f"invalid release version: {version!r}")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise CandidateError("commit must be a full lowercase 40-character SHA-1")
    if source_ref != f"refs/tags/v{version}":
        raise CandidateError(
            f"source ref {source_ref!r} does not exactly match refs/tags/v{version}"
        )


def build_documents(args: argparse.Namespace) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]]:
    validate_identity(args.version, args.commit, args.source_ref)
    artifacts = collect_artifacts(args.artifacts)
    archives = [entry for entry in artifacts if entry["kind"] == "runtime_archive"]
    raw_binaries = [entry for entry in artifacts if entry["kind"] == "runtime_binary"]
    sboms = [entry for entry in artifacts if entry["kind"] == "sbom_spdx_json"]
    ui_provenance = [
        entry for entry in artifacts if entry["kind"] == "ui_bundle_provenance"
    ]
    npm_packages = [
        entry for entry in artifacts if entry["kind"] == "npm_package_tarball"
    ]
    expected_targets = sorted(set(args.expected_target))
    if len(sboms) != 1:
        raise CandidateError(f"expected exactly one {SBOM_NAME}; found {len(sboms)}")
    if len(ui_provenance) != 1:
        raise CandidateError(
            f"expected exactly one {UI_BUNDLE_PROVENANCE_NAME}; found {len(ui_provenance)}"
        )
    if ui_provenance[0]["source_commit"] != args.commit:
        raise CandidateError("UI bundle provenance is not bound to the candidate commit")
    if len(npm_packages) != 1:
        raise CandidateError(f"expected exactly one npm package tarball; found {len(npm_packages)}")
    if npm_packages[0]["package_version"] != args.version:
        raise CandidateError("npm package version is not bound to the candidate version")
    cargo_packages = bind_crate_packages(
        artifacts=artifacts,
        release_version=args.version,
        commit=args.commit,
        ui_provenance=ui_provenance[0],
    )
    runtime_bindings = bind_runtime_artifacts(
        directory=args.artifacts,
        artifacts=artifacts,
        expected_targets=expected_targets,
        version=args.version,
        commit=args.commit,
        ui_bundle_sha256=ui_provenance[0]["ui_bundle_sha256"],
    )

    seed = candidate_seed(
        version=args.version,
        commit=args.commit,
        source_ref=args.source_ref,
        artifacts=artifacts,
        runtime_bindings=runtime_bindings,
    )
    candidate_id = f"sha256:{sha256_bytes(canonical_json(seed))}"
    manifest = {
        "schema": SCHEMA,
        "candidate_id": candidate_id,
        **seed,
        "cargo_packages": cargo_packages,
        "npm_package": npm_packages[0],
        "build_policy": {
            "builds_per_target": 1,
            "cargo_packages_per_crate": 1,
            "cargo_source_overlay": "clean-tag-plus-candidate-sealed-mcp-ui",
            "archive_raw_digest_match": True,
            "promotion": "exact_declared_bytes_only",
            "raw_asset_install": True,
            "targets": expected_targets,
        },
    }
    manifest_digest = sha256_bytes(canonical_json(manifest))
    gate_receipt = {
        "schema": GATE_SCHEMA,
        "candidate_id": candidate_id,
        "candidate_manifest_sha256": manifest_digest,
        "commit": args.commit,
        "decision": "PASS",
        "required_upstream_jobs": sorted(set(args.required_job)),
        "source_ref": args.source_ref,
        "version": args.version,
        "workflow_run_id": str(args.run_id),
    }
    rollback = {
        "schema": ROLLBACK_SCHEMA,
        "candidate_id": candidate_id,
        "commit": args.commit,
        "archive_artifacts": archives,
        "runtime_artifacts": raw_binaries,
        "runtime_bindings": runtime_bindings,
        "activation": {
            "automatic": False,
            "command": "m1nd update apply --yes",
        },
        "rollback": {
            "automatic": False,
            "command": "m1nd update rollback",
            "requires_local_state_schema": "m1nd-self-update-rollback-state-v0",
            "source": "pre-activation local runtime backup",
        },
        "version": args.version,
    }
    return manifest, gate_receipt, rollback


def assemble(args: argparse.Namespace) -> None:
    manifest, receipt, rollback = build_documents(args)
    atomic_json(args.output, manifest)
    atomic_json(args.receipt_output, receipt)
    atomic_json(args.rollback_output, rollback)


def verify(args: argparse.Namespace) -> None:
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    if manifest.get("schema") != SCHEMA:
        raise CandidateError(f"unexpected candidate schema: {manifest.get('schema')!r}")
    required = {
        "version",
        "commit",
        "source_ref",
        "artifacts",
        "candidate_id",
        "build_policy",
        "runtime_bindings",
        "cargo_packages",
        "npm_package",
    }
    missing = sorted(required - manifest.keys())
    if missing:
        raise CandidateError(f"candidate manifest missing fields: {missing}")
    validate_identity(manifest["version"], manifest["commit"], manifest["source_ref"])
    observed = collect_artifacts(args.artifacts, allow_derived_controls=True)
    if observed != manifest["artifacts"]:
        raise CandidateError("artifact bytes or declared file set differ from candidate manifest")
    expected_targets = manifest["build_policy"].get("targets")
    if not isinstance(expected_targets, list) or not all(
        isinstance(target, str) for target in expected_targets
    ):
        raise CandidateError("candidate build policy lacks a valid target list")
    if manifest["build_policy"].get("cargo_packages_per_crate") != 1:
        raise CandidateError("candidate did not declare exactly one package build per crate")
    if manifest["build_policy"].get("cargo_source_overlay") != (
        "clean-tag-plus-candidate-sealed-mcp-ui"
    ):
        raise CandidateError("candidate Cargo source overlay policy drifted")
    ui_provenance = [
        entry for entry in observed if entry["kind"] == "ui_bundle_provenance"
    ]
    if len(ui_provenance) != 1:
        raise CandidateError("candidate must contain exactly one UI bundle provenance")
    if ui_provenance[0]["source_commit"] != manifest["commit"]:
        raise CandidateError("candidate UI provenance commit mismatch")
    npm_packages = [
        entry for entry in observed if entry["kind"] == "npm_package_tarball"
    ]
    if len(npm_packages) != 1 or npm_packages[0] != manifest["npm_package"]:
        raise CandidateError("candidate npm package binding mismatch")
    if npm_packages[0]["package_version"] != manifest["version"]:
        raise CandidateError("candidate npm package version mismatch")
    ui_provenance = [
        entry for entry in observed if entry["kind"] == "ui_bundle_provenance"
    ]
    observed_cargo_packages = bind_crate_packages(
        artifacts=observed,
        release_version=manifest["version"],
        commit=manifest["commit"],
        ui_provenance=ui_provenance[0],
    )
    if observed_cargo_packages != manifest["cargo_packages"]:
        raise CandidateError("candidate Cargo package binding mismatch")
    observed_bindings = bind_runtime_artifacts(
        directory=args.artifacts,
        artifacts=observed,
        expected_targets=expected_targets,
        version=manifest["version"],
        commit=manifest["commit"],
        ui_bundle_sha256=ui_provenance[0]["ui_bundle_sha256"],
    )
    if observed_bindings != manifest["runtime_bindings"]:
        raise CandidateError("runtime archive/raw bindings differ from candidate manifest")
    seed = candidate_seed(
        version=manifest["version"],
        commit=manifest["commit"],
        source_ref=manifest["source_ref"],
        artifacts=manifest["artifacts"],
        runtime_bindings=manifest["runtime_bindings"],
    )
    expected_id = f"sha256:{sha256_bytes(canonical_json(seed))}"
    if manifest["candidate_id"] != expected_id:
        raise CandidateError(
            f"candidate id mismatch: expected={expected_id}, actual={manifest['candidate_id']}"
        )


def verify_update_receipts(args: argparse.Namespace) -> None:
    manifest = load_json(args.manifest, "candidate manifest")
    if manifest.get("schema") != SCHEMA:
        raise CandidateError(f"unexpected candidate schema: {manifest.get('schema')!r}")
    expected_targets = manifest.get("build_policy", {}).get("targets")
    if not isinstance(expected_targets, list) or not all(
        isinstance(target, str) for target in expected_targets
    ):
        raise CandidateError("candidate build policy lacks a valid target list")
    bindings = {}
    for binding in manifest.get("runtime_bindings", []):
        if not isinstance(binding, dict) or not isinstance(binding.get("target"), str):
            raise CandidateError("candidate contains an invalid runtime binding")
        if binding["target"] in bindings:
            raise CandidateError(f"duplicate runtime binding for {binding['target']}")
        bindings[binding["target"]] = binding
    if not args.receipts.is_dir():
        raise CandidateError(f"verified updater receipt directory does not exist: {args.receipts}")
    actual_paths = []
    for path in sorted(args.receipts.iterdir()):
        if path.is_symlink():
            raise CandidateError(f"symlink updater receipt refused: {path.name}")
        if not path.is_file():
            continue
        if not VERIFIED_UPDATE_RECEIPT_RE.fullmatch(path.name):
            raise CandidateError(f"unrecognized updater receipt refused: {path.name}")
        actual_paths.append(path)
    actual_targets = sorted(
        VERIFIED_UPDATE_RECEIPT_RE.fullmatch(path.name).group("target")
        for path in actual_paths
    )
    expected_targets = sorted(set(expected_targets))
    if actual_targets != expected_targets:
        raise CandidateError(
            f"verified updater receipt target set mismatch: expected={expected_targets}, actual={actual_targets}"
        )
    manifest_sha256 = sha256_file(args.manifest)
    expected_identity = (
        "https://github.com/maxkle1nz/m1nd/.github/workflows/"
        f"release.yml@refs/tags/v{manifest['version']}"
    )
    expected_issuer = "https://token.actions.githubusercontent.com"
    for path in actual_paths:
        match = VERIFIED_UPDATE_RECEIPT_RE.fullmatch(path.name)
        assert match
        target = match.group("target")
        receipt = load_json(path, "verified updater receipt")
        proofs = receipt.get("proofs")
        verification = receipt.get("candidate_verification")
        overrides = receipt.get("test_overrides")
        binding = bindings.get(target)
        if binding is None:
            raise CandidateError(f"candidate lacks runtime binding for updater receipt {target}")
        if (
            receipt.get("schema") != "m1nd-release-verified-update-smoke-v2"
            or receipt.get("target") != target
            or receipt.get("verdict") != "PASS"
            or receipt.get("candidate_id") != manifest.get("candidate_id")
            or receipt.get("candidate_manifest_sha256") != manifest_sha256
            or not isinstance(proofs, dict)
            or any(proofs.get(proof) is not True for proof in VERIFIED_UPDATE_PROOFS)
            or not isinstance(verification, dict)
            or not isinstance(overrides, dict)
        ):
            raise CandidateError(f"invalid or incomplete verified updater receipt: {path.name}")
        if (
            verification.get("candidate_id") != manifest.get("candidate_id")
            or verification.get("manifest_sha256") != manifest_sha256
            or verification.get("target") != target
            or verification.get("raw_sha256") != binding.get("runtime_sha256")
            or verification.get("raw_size_bytes") != binding.get("size_bytes")
            or verification.get("certificate_identity") != expected_identity
            or verification.get("certificate_oidc_issuer") != expected_issuer
            or verification.get("verifier_source") != "trusted-fixed-path"
            or verification.get("transport_source") != "local-test-directory"
        ):
            raise CandidateError(
                f"verified updater receipt is not bound to the signed candidate/runtime for {target}"
            )
        if (
            overrides.get("active") is not True
            or overrides.get("release_transport") != "local-test-directory"
            or overrides.get("verifier_source") != "trusted-fixed-path"
        ):
            raise CandidateError(
                f"verified updater receipt does not disclose its CI-local transport seam: {path.name}"
            )


def _canonical_sha256(value: Any) -> str:
    return sha256_bytes(canonical_release.canonical_json(value))


def _require_exact_object(
    value: Any, expected_fields: set[str], description: str
) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise CandidateError(f"{description} must be a JSON object")
    if set(value) != expected_fields:
        raise CandidateError(
            f"{description} fields differ: "
            f"missing={sorted(expected_fields - set(value))}, "
            f"unknown={sorted(set(value) - expected_fields)}"
        )
    return value


def validate_canonical_compatibility(value: Any) -> dict[str, Any]:
    manifest = _require_exact_object(
        value,
        {"schema", "version", "commit", "source_ref", "targets"},
        "canonical compatibility manifest",
    )
    if manifest["schema"] != CANONICAL_COMPATIBILITY_SCHEMA:
        raise CandidateError("unexpected canonical compatibility schema")
    validate_identity(manifest["version"], manifest["commit"], manifest["source_ref"])
    if not isinstance(manifest["targets"], list) or not manifest["targets"]:
        raise CandidateError("canonical compatibility targets must be a non-empty array")
    seen: set[str] = set()
    for item in manifest["targets"]:
        target = _require_exact_object(
            item,
            {"target", "asset", "sha256", "size_bytes"},
            "canonical compatibility target",
        )
        if (
            not isinstance(target["target"], str)
            or re.fullmatch(r"[a-z0-9_-]+", target["target"]) is None
        ):
            raise CandidateError("canonical compatibility target name is invalid")
        if target["target"] in seen:
            raise CandidateError(f"duplicate canonical compatibility target {target['target']}")
        seen.add(target["target"])
        if target["asset"] != raw_asset_name(target["target"]):
            raise CandidateError(
                f"canonical compatibility asset mismatch for {target['target']}"
            )
        require_sha256(target["sha256"], f"canonical target {target['target']} digest")
        if (
            isinstance(target["size_bytes"], bool)
            or not isinstance(target["size_bytes"], int)
            or target["size_bytes"] <= 0
            or target["size_bytes"] > 2**64 - 1
        ):
            raise CandidateError(
                f"canonical compatibility size is not a positive u64 for {target['target']}"
            )
    return manifest


def validate_canonical_rollback(value: Any) -> dict[str, Any]:
    plan = _require_exact_object(
        value,
        {
            "schema",
            "version",
            "commit",
            "source_ref",
            "runtime_bindings",
            "activation",
            "rollback",
        },
        "canonical rollback plan",
    )
    if plan["schema"] != CANONICAL_ROLLBACK_SCHEMA:
        raise CandidateError("unexpected canonical rollback schema")
    validate_identity(plan["version"], plan["commit"], plan["source_ref"])
    if not isinstance(plan["runtime_bindings"], list) or not plan["runtime_bindings"]:
        raise CandidateError("canonical rollback plan requires runtime bindings")
    seen_targets: set[str] = set()
    for item in plan["runtime_bindings"]:
        binding = _require_exact_object(
            item,
            {
                "archive",
                "archive_member",
                "artifact_smoke_receipt",
                "raw_binary",
                "runtime_sha256",
                "size_bytes",
                "target",
            },
            "canonical rollback runtime binding",
        )
        target = binding["target"]
        if not isinstance(target, str) or re.fullmatch(r"[a-z0-9_-]+", target) is None:
            raise CandidateError("canonical rollback target is invalid")
        if target in seen_targets:
            raise CandidateError(f"duplicate canonical rollback target {target}")
        seen_targets.add(target)
        expected_archive = f"m1nd-mcp-{target}.{'zip' if target.startswith('windows-') else 'tar.gz'}"
        expected_receipt = f"GATE-ARTIFACT-SMOKE-{target}.json"
        if binding["archive"] != expected_archive:
            raise CandidateError(f"canonical rollback archive mismatch for {target}")
        if binding["archive_member"] != expected_archive_member(target):
            raise CandidateError(f"canonical rollback archive member mismatch for {target}")
        if binding["artifact_smoke_receipt"] != expected_receipt:
            raise CandidateError(f"canonical rollback smoke receipt mismatch for {target}")
        if binding["raw_binary"] != raw_asset_name(target):
            raise CandidateError(f"canonical rollback raw runtime mismatch for {target}")
        require_sha256(binding["runtime_sha256"], f"canonical rollback {target} digest")
        if (
            isinstance(binding["size_bytes"], bool)
            or not isinstance(binding["size_bytes"], int)
            or binding["size_bytes"] <= 0
            or binding["size_bytes"] > 2**64 - 1
        ):
            raise CandidateError(f"canonical rollback size is not a positive u64 for {target}")
    activation = _require_exact_object(
        plan["activation"], {"automatic", "command"}, "canonical activation plan"
    )
    rollback = _require_exact_object(
        plan["rollback"],
        {"automatic", "command", "requires_local_state_schema", "source"},
        "canonical rollback action",
    )
    if activation != {"automatic": False, "command": "m1nd update apply --yes"}:
        raise CandidateError("canonical activation must remain explicit and non-automatic")
    if rollback.get("automatic") is not False or rollback.get("command") != "m1nd update rollback":
        raise CandidateError("canonical rollback must remain explicit and non-automatic")
    if rollback.get("requires_local_state_schema") != "m1nd-self-update-rollback-state-v0":
        raise CandidateError("canonical rollback state schema drifted")
    if rollback.get("source") != "pre-activation local runtime backup":
        raise CandidateError("canonical rollback source drifted")
    return plan


def validate_canonical_operational_pair(
    compatibility: dict[str, Any], rollback: dict[str, Any]
) -> None:
    for field in ("version", "commit", "source_ref"):
        if compatibility[field] != rollback[field]:
            raise CandidateError(f"compatibility and rollback {field} differ")
    compatibility_by_target = {
        target["target"]: target for target in compatibility["targets"]
    }
    rollback_by_target = {
        binding["target"]: binding for binding in rollback["runtime_bindings"]
    }
    if set(compatibility_by_target) != set(rollback_by_target):
        raise CandidateError("compatibility and rollback target sets differ")
    for target, compatible in compatibility_by_target.items():
        binding = rollback_by_target[target]
        if (
            binding["raw_binary"] != compatible["asset"]
            or binding["runtime_sha256"] != compatible["sha256"]
            or binding["size_bytes"] != compatible["size_bytes"]
        ):
            raise CandidateError(
                f"compatibility and rollback runtime bytes differ for {target}"
            )


def require_exact_canonical_file(path: Path, value: Any, description: str) -> None:
    if path.read_bytes() != canonical_release.canonical_json(value):
        raise CandidateError(
            f"{description} is not exact canonical UTF-8/no-newline JSON"
        )


def prepare_canonical_operational(args: argparse.Namespace) -> None:
    """Prepare non-circular updater/rollback inputs before candidate sealing."""

    validate_identity(args.version, args.commit, args.source_ref)
    artifacts = collect_artifacts(args.artifacts)
    sboms = [entry for entry in artifacts if entry["kind"] == "sbom_spdx_json"]
    if len(sboms) != 1:
        raise CandidateError(f"expected exactly one {SBOM_NAME}; found {len(sboms)}")
    ui_provenance = [
        entry for entry in artifacts if entry["kind"] == "ui_bundle_provenance"
    ]
    npm_packages = [
        entry for entry in artifacts if entry["kind"] == "npm_package_tarball"
    ]
    if len(ui_provenance) != 1:
        raise CandidateError(
            f"expected exactly one {UI_BUNDLE_PROVENANCE_NAME}; found {len(ui_provenance)}"
        )
    if ui_provenance[0]["source_commit"] != args.commit:
        raise CandidateError("UI bundle provenance is not bound to the candidate commit")
    if len(npm_packages) != 1:
        raise CandidateError(f"expected exactly one npm package tarball; found {len(npm_packages)}")
    if npm_packages[0]["package_version"] != args.version:
        raise CandidateError("npm package version is not bound to the candidate version")
    bind_crate_packages(
        artifacts=artifacts,
        release_version=args.version,
        commit=args.commit,
        ui_provenance=ui_provenance[0],
    )
    expected_targets = sorted(set(args.expected_target))
    runtime_bindings = bind_runtime_artifacts(
        directory=args.artifacts,
        artifacts=artifacts,
        expected_targets=expected_targets,
        version=args.version,
        commit=args.commit,
        ui_bundle_sha256=ui_provenance[0]["ui_bundle_sha256"],
    )
    raw_by_target = unique_target_map(artifacts, "runtime_binary")
    compatibility = {
        "schema": CANONICAL_COMPATIBILITY_SCHEMA,
        "version": args.version,
        "commit": args.commit,
        "source_ref": args.source_ref,
        "targets": [
            {
                "target": target,
                "asset": raw_by_target[target]["name"],
                "sha256": raw_by_target[target]["sha256"],
                "size_bytes": raw_by_target[target]["size_bytes"],
            }
            for target in expected_targets
        ],
    }
    rollback = {
        "schema": CANONICAL_ROLLBACK_SCHEMA,
        "version": args.version,
        "commit": args.commit,
        "source_ref": args.source_ref,
        "runtime_bindings": runtime_bindings,
        "activation": {"automatic": False, "command": "m1nd update apply --yes"},
        "rollback": {
            "automatic": False,
            "command": "m1nd update rollback",
            "requires_local_state_schema": "m1nd-self-update-rollback-state-v0",
            "source": "pre-activation local runtime backup",
        },
    }
    validate_canonical_compatibility(compatibility)
    validate_canonical_rollback(rollback)
    validate_canonical_operational_pair(compatibility, rollback)
    compatibility_digest = _canonical_sha256(compatibility)
    rollback_digest = _canonical_sha256(rollback)
    artifact_digests = {
        canonical_release.COMPATIBILITY_ARTIFACT_KEY: compatibility_digest,
        canonical_release.ROLLBACK_ARTIFACT_KEY: rollback_digest,
    }
    artifact_digests.update(
        {
            f"{canonical_release.RELEASE_ARTIFACT_PREFIX}{entry['name']}": entry["sha256"]
            for entry in artifacts
        }
    )
    artifact_digests.update(
        {
            f"{canonical_release.RELEASE_ASSET_ARTIFACT_PREFIX}{entry['name']}": entry[
                "sha256"
            ]
            for entry in raw_by_target.values()
        }
    )
    descriptor = {
        "schema": CANONICAL_OPERATIONAL_DIGESTS_SCHEMA,
        "artifact_digests": artifact_digests,
        "compatibility_manifest_digest": compatibility_digest,
        "rollback_plan_digest": rollback_digest,
    }
    atomic_canonical_json(args.compatibility_output, compatibility)
    atomic_canonical_json(args.rollback_output, rollback)
    atomic_canonical_json(args.digest_output, descriptor)


def _contains_placeholder(value: Any) -> bool:
    if isinstance(value, str):
        normalized = value.strip().upper().replace("-", "_")
        return normalized in {"TODO", "TBD", "UNKNOWN", "PLACEHOLDER", "NOT_PROVEN"}
    if isinstance(value, list):
        return any(_contains_placeholder(item) for item in value)
    if isinstance(value, dict):
        return any(_contains_placeholder(item) for item in value.values())
    return False


def _canonical_core_input(path: Path, *, fixture_only: bool) -> dict[str, Any]:
    wrapper = canonical_release.load_integer_json(path, "canonical core input")
    wrapper = _require_exact_object(
        wrapper, {"schema", "authority", "core"}, "canonical core input"
    )
    if wrapper["schema"] != CANONICAL_CORE_INPUT_SCHEMA:
        raise CandidateError("unexpected canonical core-input schema")
    authority = _require_exact_object(
        wrapper["authority"],
        {"authority_class", "producer_id", "producer_key_version", "authority_receipt_digest"},
        "canonical core-input authority",
    )
    expected_classes = {"FIXTURE_ONLY"} if fixture_only else {"HUMAN_RATIFIED", "GOVERNANCE_QUORUM"}
    if authority["authority_class"] not in expected_classes:
        raise CandidateError(
            f"canonical core input authority must be one of {sorted(expected_classes)}"
        )
    for field in ("producer_id", "producer_key_version"):
        if not isinstance(authority[field], str) or not authority[field].strip():
            raise CandidateError(f"canonical core-input authority field {field} is empty")
    require_sha256(
        authority["authority_receipt_digest"], "canonical core-input authority receipt"
    )
    if _contains_placeholder(wrapper["core"]):
        raise CandidateError("canonical core input contains a placeholder or NOT_PROVEN value")
    return canonical_release.validate_candidate_core(wrapper["core"])


def _require_builder_signature(signature: str, fixture_only: bool) -> None:
    if not signature:
        raise CandidateError("structural signature is empty")
    marked = signature.startswith(canonical_release.FIXTURE_SIGNATURE_PREFIX)
    if fixture_only and not marked:
        raise CandidateError(
            f"fixture signatures must start {canonical_release.FIXTURE_SIGNATURE_PREFIX}"
        )
    if not fixture_only and marked:
        raise CandidateError("NOT_CRYPTOGRAPHIC signatures are forbidden outside fixture-only mode")


def seal_canonical_candidate(args: argparse.Namespace) -> None:
    core = _canonical_core_input(args.core_input, fixture_only=args.fixture_only)
    canonical_release.validate_operational_artifact_keys(core)
    _require_builder_signature(args.provenance_signature, args.fixture_only)
    compatibility = validate_canonical_compatibility(
        canonical_release.load_integer_json(
            args.compatibility_manifest, "canonical compatibility manifest"
        )
    )
    rollback = validate_canonical_rollback(
        canonical_release.load_integer_json(args.rollback_plan, "canonical rollback plan")
    )
    require_exact_canonical_file(
        args.compatibility_manifest, compatibility, "canonical compatibility manifest"
    )
    require_exact_canonical_file(args.rollback_plan, rollback, "canonical rollback plan")
    validate_canonical_operational_pair(compatibility, rollback)
    compatibility_digest = sha256_file(args.compatibility_manifest)
    rollback_digest = sha256_file(args.rollback_plan)
    if core["compatibility_manifest_digest"] != compatibility_digest:
        raise CandidateError("canonical core does not bind exact compatibility bytes")
    if core["rollback_plan_digest"] != rollback_digest:
        raise CandidateError("canonical core does not bind exact rollback bytes")
    commit = compatibility["commit"]
    if core["repo_commits"].get("m1nd") != commit:
        raise CandidateError("canonical core repo_commits.m1nd does not match compatibility commit")
    candidate = canonical_release.seal_candidate(core, args.provenance_signature)
    atomic_canonical_json(args.output, candidate)


def verify_canonical_candidate(args: argparse.Namespace) -> None:
    candidate = canonical_release.validate_candidate(
        canonical_release.load_integer_json(args.manifest, "canonical candidate")
    )
    canonical_release.validate_operational_artifact_keys(candidate["core"])
    compatibility = None
    rollback = None
    if args.compatibility_manifest is not None:
        compatibility = validate_canonical_compatibility(
            canonical_release.load_integer_json(
                args.compatibility_manifest, "canonical compatibility manifest"
            )
        )
        require_exact_canonical_file(
            args.compatibility_manifest,
            compatibility,
            "canonical compatibility manifest",
        )
        if sha256_file(args.compatibility_manifest) != candidate["core"][
            "compatibility_manifest_digest"
        ]:
            raise CandidateError("canonical candidate compatibility digest mismatch")
        if candidate["core"]["repo_commits"].get("m1nd") != compatibility["commit"]:
            raise CandidateError("canonical candidate/compatibility commit mismatch")
    if args.rollback_plan is not None:
        rollback = validate_canonical_rollback(
            canonical_release.load_integer_json(args.rollback_plan, "canonical rollback plan")
        )
        require_exact_canonical_file(
            args.rollback_plan, rollback, "canonical rollback plan"
        )
        if sha256_file(args.rollback_plan) != candidate["core"]["rollback_plan_digest"]:
            raise CandidateError("canonical candidate rollback digest mismatch")
    if compatibility is not None and rollback is not None:
        validate_canonical_operational_pair(compatibility, rollback)
    print(canonical_release.STRUCTURAL_STATUS)


def seal_canonical_gate(args: argparse.Namespace) -> None:
    core = canonical_release.load_integer_json(args.core, "canonical gate core")
    _require_builder_signature(args.signature, args.fixture_only)
    validated = canonical_release.validate_gate_core(core)
    if validated["gate_id"] == "G8" and validated["verdict"] == "PASS":
        actual = set(validated["artifact_digests"])
        missing = sorted(G8_REQUIRED_ARTIFACT_KEYS - actual)
        if missing:
            raise CandidateError(
                "G8 PASS cannot be sealed from updater smokes alone; "
                f"missing full G8 evidence keys: {missing}"
            )
    atomic_canonical_json(
        args.output, canonical_release.seal_gate_receipt(validated, args.signature)
    )


def seal_canonical_review(args: argparse.Namespace) -> None:
    core = canonical_release.load_integer_json(args.core, "canonical review core")
    _require_builder_signature(args.signature, args.fixture_only)
    atomic_canonical_json(
        args.output, canonical_release.seal_independent_review(core, args.signature)
    )


def verify_canonical_evidence(args: argparse.Namespace) -> None:
    candidate = canonical_release.load_integer_json(args.candidate, "canonical candidate")
    review = canonical_release.load_integer_json(args.review, "canonical review receipt")
    if not args.receipts.is_dir():
        raise CandidateError(f"canonical gate receipt directory is missing: {args.receipts}")
    paths = sorted(path for path in args.receipts.iterdir() if path.is_file())
    expected_names = {f"{gate}.json" for gate in canonical_release.GATE_IDS}
    actual_names = {path.name for path in paths}
    if actual_names != expected_names:
        raise CandidateError(
            f"canonical receipt file set mismatch: expected={sorted(expected_names)}, "
            f"actual={sorted(actual_names)}"
        )
    receipts = [
        canonical_release.load_integer_json(
            args.receipts / f"{gate}.json", f"canonical {gate} receipt"
        )
        for gate in canonical_release.GATE_IDS
    ]
    evidence = canonical_release.evidence_set_json_extension(candidate, receipts, review)
    if args.output is not None:
        atomic_canonical_json(args.output, evidence)
    print(canonical_release.STRUCTURAL_STATUS)


def verify_canonical_vectors(args: argparse.Namespace) -> None:
    vectors = canonical_release.verify_vectors(
        canonical_release.load_integer_json(args.vectors, "cross-language vectors")
    )
    operational = vectors["operational_manifests"]
    compatibility = validate_canonical_compatibility(operational["compatibility"])
    rollback = validate_canonical_rollback(operational["rollback"])
    validate_canonical_operational_pair(compatibility, rollback)
    print(canonical_release.STRUCTURAL_STATUS)


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    commands = root.add_subparsers(dest="command", required=True)

    create = commands.add_parser("assemble")
    create.add_argument("--artifacts", type=Path, required=True)
    create.add_argument("--version", required=True)
    create.add_argument("--commit", required=True)
    create.add_argument("--source-ref", required=True)
    create.add_argument("--run-id", required=True)
    create.add_argument("--expected-target", action="append", default=[], required=True)
    create.add_argument("--required-job", action="append", default=[], required=True)
    create.add_argument("--output", type=Path, required=True)
    create.add_argument("--receipt-output", type=Path, required=True)
    create.add_argument("--rollback-output", type=Path, required=True)
    create.set_defaults(run=assemble)

    check = commands.add_parser("verify")
    check.add_argument("--artifacts", type=Path, required=True)
    check.add_argument("--manifest", type=Path, required=True)
    check.set_defaults(run=verify)

    updater = commands.add_parser("verify-update-receipts")
    updater.add_argument("--receipts", type=Path, required=True)
    updater.add_argument("--manifest", type=Path, required=True)
    updater.set_defaults(run=verify_update_receipts)

    prepare = commands.add_parser("prepare-canonical-operational")
    prepare.add_argument("--artifacts", type=Path, required=True)
    prepare.add_argument("--version", required=True)
    prepare.add_argument("--commit", required=True)
    prepare.add_argument("--source-ref", required=True)
    prepare.add_argument("--expected-target", action="append", default=[], required=True)
    prepare.add_argument("--compatibility-output", type=Path, required=True)
    prepare.add_argument("--rollback-output", type=Path, required=True)
    prepare.add_argument("--digest-output", type=Path, required=True)
    prepare.set_defaults(run=prepare_canonical_operational)

    seal_candidate = commands.add_parser("seal-canonical-candidate")
    seal_candidate.add_argument("--core-input", type=Path, required=True)
    seal_candidate.add_argument("--compatibility-manifest", type=Path, required=True)
    seal_candidate.add_argument("--rollback-plan", type=Path, required=True)
    seal_candidate.add_argument("--provenance-signature", required=True)
    seal_candidate.add_argument("--fixture-only", action="store_true")
    seal_candidate.add_argument("--output", type=Path, required=True)
    seal_candidate.set_defaults(run=seal_canonical_candidate)

    check_candidate = commands.add_parser("verify-canonical-candidate")
    check_candidate.add_argument("--manifest", type=Path, required=True)
    check_candidate.add_argument("--compatibility-manifest", type=Path)
    check_candidate.add_argument("--rollback-plan", type=Path)
    check_candidate.set_defaults(run=verify_canonical_candidate)

    seal_gate = commands.add_parser("seal-canonical-gate")
    seal_gate.add_argument("--core", type=Path, required=True)
    seal_gate.add_argument("--signature", required=True)
    seal_gate.add_argument("--fixture-only", action="store_true")
    seal_gate.add_argument("--output", type=Path, required=True)
    seal_gate.set_defaults(run=seal_canonical_gate)

    seal_review = commands.add_parser("seal-canonical-review")
    seal_review.add_argument("--core", type=Path, required=True)
    seal_review.add_argument("--signature", required=True)
    seal_review.add_argument("--fixture-only", action="store_true")
    seal_review.add_argument("--output", type=Path, required=True)
    seal_review.set_defaults(run=seal_canonical_review)

    check_evidence = commands.add_parser("verify-canonical-evidence")
    check_evidence.add_argument("--candidate", type=Path, required=True)
    check_evidence.add_argument("--receipts", type=Path, required=True)
    check_evidence.add_argument("--review", type=Path, required=True)
    check_evidence.add_argument("--output", type=Path)
    check_evidence.set_defaults(run=verify_canonical_evidence)

    vectors = commands.add_parser("verify-canonical-vectors")
    vectors.add_argument("--vectors", type=Path, required=True)
    vectors.set_defaults(run=verify_canonical_vectors)
    return root


def main() -> int:
    args = parser().parse_args()
    try:
        args.run(args)
    except (
        CandidateError,
        canonical_release.ReleaseContractError,
        OSError,
        json.JSONDecodeError,
    ) as error:
        print(f"release candidate refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
