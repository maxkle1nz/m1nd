#!/usr/bin/env python3
"""Fail-closed scorer for the M1ND-10 G6 held-out retrieval gate.

Version 2 deliberately refuses the historical v1 result shape. A claimable
report requires a ratified, outcome-blind MetricSpec; byte- and semantic-bound
public/sealed corpora; exact runner and binary artifacts; two distinct blinded
runs; an independently supplied baseline-ratification receipt; and one
hash-chained sealed-run ledger. Missing evidence is ``NOT_PROVEN``. A measured
threshold miss is ``FAIL``.

The scorer consumes operator labels only for scoring. It never creates labels,
ratifies a baseline, upgrades a legacy artifact, or signs a release receipt.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import pathlib
import re
import sys
from collections import Counter
from datetime import datetime, timezone
from typing import Any


REPORT_SCHEMA = "m1nd10-g6-retrieval-report-v2"
PUBLIC_SCHEMA = "m1nd10-g6-public-query-corpus-v2"
HISTORICAL_PUBLIC_SCHEMA = "m1nd10-g6-public-query-corpus-v1"
CASE_SCHEMA = "m1nd10-g6-held-out-corpus-v2"
HISTORICAL_CASE_SCHEMA = "m1nd10-g6-held-out-corpus-v1"
SOURCE_MANIFEST_SCHEMA = "m1nd10-g6-source-manifest-v2"
RESULT_SCHEMA = "m1nd10-g6-retrieval-results-v2"
SPEC_SCHEMA = "m1nd10-g6-metric-spec-v2"
RUN_METADATA_SCHEMA = "m1nd10-g6-blind-run-metadata-v2"
CALIBRATION_SCHEMA = "m1nd10-g6-calibration-run-v1"
BASELINE_RECEIPT_SCHEMA = "m1nd10-g6-baseline-ratification-receipt-v1"
RUN_LEDGER_SCHEMA = "m1nd10-g6-sealed-run-ledger-v1"
RUN_LEDGER_ENTRY_SCHEMA = "m1nd10-g6-sealed-run-entry-v1"
SEEK_CALIBRATION_RECEIPT_SCHEMA = "m1nd-seek-calibration-receipt-v1"
SEEK_CALIBRATION_SIGNAL = "envelope"
V2_SOURCE_COMMIT = "b59a1c2a1454a83164dfb4d5640c6b005154d1ee"
V2_SEALED_AT = "2026-07-19T08:00:00Z"
V2_AUTHOR_STATUS = "AUTHOR_ONLY_AWAITING_INDEPENDENT_REVIEW"

PUBLIC_TASK_FIELDS = frozenset(
    {"task_id", "repo_id", "repo_revision", "language", "repo_size_band", "query"}
)
VALID_VERDICTS = frozenset({"act", "reverify", "abstain"})
SHA256_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
PUBLIC_FIELDS = frozenset(
    {
        "schema",
        "version",
        "corpus_id",
        "corpus_digest",
        "blinded",
        "author_review_status",
        "source_manifest",
        "task_count",
        "runner_contract",
        "tasks",
        "self_digest",
    }
)
SEALED_FIELDS = frozenset(
    {
        "schema",
        "version",
        "corpus_id",
        "corpus_digest",
        "blinded",
        "adjudication_sealed_at",
        "author_review_status",
        "source_manifest",
        "counts",
        "methodology",
        "tasks",
        "self_digest",
    }
)
SOURCE_MANIFEST_FIELDS = frozenset(
    {
        "schema",
        "source_commit",
        "snapshot_kind",
        "worktree_state_excluded",
        "repos",
        "manifest_digest",
    }
)
REPO_MANIFEST_FIELDS = frozenset(
    {
        "repo_id",
        "source_root",
        "source_revision",
        "git_tree",
        "primary_language",
        "repo_size_band",
        "size_band_definition",
        "source_file_count",
        "source_line_count",
        "searched_file_count",
        "file_set_digest",
        "files",
    }
)
FILE_MANIFEST_FIELDS = frozenset({"path", "role", "bytes", "lines", "sha256"})
RUNNER_CONTRACT_FIELDS = frozenset(
    {
        "forbidden_artifact",
        "independent_review_status",
        "labels_exposed",
        "read_only_artifact",
        "result_coverage",
        "source_checkout",
    }
)
SIZE_BAND_DEFINITION = {
    "small": "fewer than 10000 source lines",
    "medium": "10000 through 99999 source lines",
    "large": "100000 or more source lines",
}
PROOF_NOT_PROVEN = "NOT_PROVEN"
INSTALLED_OWNER_PORT = 1338
RUN_METADATA_FIELDS = frozenset(
    {
        "schema",
        "lane",
        "run_id",
        "generated_at",
        "started_at",
        "transport",
        "task_count",
        "unscored",
        "score_eligible",
        "diagnostic_only",
        "proof_state",
        "formal_preflights",
        "authority_mode",
        "authority_provider_kind",
        "authority_provider_claimed_production_assembly",
        "production_authority_assembly_proven",
        "authority_assembly_id",
        "authority_assembly_digest",
        "authority_assembly_digest_verified",
        "authority_provider_executable_digest",
        "authority_owner_security_config_digest",
        "authority_key_registry_epoch",
        "authority_receipt_key_id",
        "authority_blind_boundary_kind",
        "authority_blind_boundary_proven",
        "labels_read",
        "actions_executed",
        "benchmark_task_actions_executed",
        "governed_setup_mutations_executed",
        "verdict_mapping",
        "raw_runtime_verdict_counts",
        "calibration",
        "source_verification",
        "post_ingest_source_verification",
        "owner_topology",
        "owner_cleanup",
        "governed_graph_ingest",
        "warmup",
        "errors",
    }
)
FORMAL_PREFLIGHT_FIELDS = frozenset(
    {
        "complete",
        "status",
        "missing",
        "delivery",
        "same_session_readiness_ingest_measurement_delete",
        "process_group_cleanup",
        "source_live_identity",
        "source_post_ingest_identity",
        "authority_blind_boundary",
        "owner_readiness_bindings_proven",
        "path_topology",
        "authority_receipts_proven",
        "checkpoint",
    }
)
PATH_TOPOLOGY_PROOF_FIELDS = frozenset(
    {
        "absolute",
        "fresh_mutable_roots",
        "disjoint",
        "symlink_free_path_components",
        "paths",
    }
)
OWNER_TOPOLOGY_FIELDS = frozenset(
    {
        "repo_id",
        "owner_id",
        "instance_id",
        "source_revision",
        "file_set_digest",
        "source_root",
        "port",
        "runtime_dir",
        "registry_dir",
        "process_isolated",
        "mcp_session_isolated",
        "readiness",
        "mcp_session_id",
        "cleanup",
    }
)
OWNER_READINESS_FIELDS = frozenset(
    {
        "pid",
        "started_at_ms",
        "registry_entry_digest",
        "manifest_digest",
        "binary_digest",
        "token_captured_once",
        "owner_binding_proven",
    }
)
OWNER_CLEANUP_FIELDS = frozenset(
    {
        "repo_id",
        "same_session_for_owner_lifetime",
        "session_delete_proven",
        "process_group_terminated",
        "cleanup_complete",
    }
)
GOVERNED_INGEST_FIELDS = frozenset(
    {
        "repo_id",
        "owner_id",
        "source_revision",
        "file_set_digest",
        "semantic_payload_digest",
        "operation_object_digest",
        "mcp_session_id",
        "candidate_ownership_digest",
        "candidate_source_projection_digest",
        "candidate_pipeline_digest",
        "authorization_lease_bound",
        "authority_receipt",
        "production_authority_receipt_proven",
        "reconciliation_state",
        "files_scanned",
        "files_parsed",
        "node_count",
        "edge_count",
        "mutation_proof",
        "governed_ingest_latency_ms",
    }
)
AUTHORITY_RECEIPT_PROOF_FIELDS = frozenset(
    {
        "authority_variant",
        "control_verified_ed25519",
        "receipt_core_digest_verified",
        "assembly_digest_verified",
        "key_registry_epoch",
        "signature_verified",
        "clock_verified",
        "key_lifecycle_verified",
        "checked_at_ms",
        "receipt_signer_metadata_production",
        "production_authority_receipt_proven",
        "receipt_digest",
        "issuer",
        "key_id",
        "algorithm",
    }
)
SOURCE_VERIFICATION_FIELDS = frozenset(
    {
        "checked_files",
        "missing_files",
        "digest_mismatches",
        "extra_files",
        "checked_bytes",
        "checked_lines",
        "exact_live_file_set",
        "symlinks_rejected",
        "isolated_snapshot_required",
        "git_objects_used_as_live_root",
        "repo_roots",
    }
)


def _load(path: pathlib.Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def _canonical_bytes(value: Any) -> bytes:
    return json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")


def _sha256_bytes(value: bytes) -> str:
    return "sha256:" + hashlib.sha256(value).hexdigest()


def _sha256_path(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def _without_key(value: dict[str, Any], key: str) -> dict[str, Any]:
    return {name: item for name, item in value.items() if name != key}


def _self_digest(value: dict[str, Any]) -> str:
    return _sha256_bytes(_canonical_bytes(_without_key(value, "self_digest")))


def _entry_digest(value: dict[str, Any]) -> str:
    return _sha256_bytes(_canonical_bytes(_without_key(value, "entry_digest")))


def _is_digest(value: Any) -> bool:
    return isinstance(value, str) and SHA256_RE.fullmatch(value) is not None


def _size_band(lines: int) -> str:
    if lines < 10_000:
        return "small"
    if lines < 100_000:
        return "medium"
    return "large"


def _validate_source_manifest_contract(manifest: Any) -> list[str]:
    """Validate the closed, immutable held-out-v2 manifest contract."""
    blockers: list[str] = []
    if not isinstance(manifest, dict):
        return ["source manifest is absent"]
    if set(manifest) != SOURCE_MANIFEST_FIELDS:
        blockers.append("source manifest violates its closed v2 field set")
    if (
        manifest.get("schema") != SOURCE_MANIFEST_SCHEMA
        or manifest.get("source_commit") != V2_SOURCE_COMMIT
        or manifest.get("snapshot_kind") != "immutable_git_objects"
        or manifest.get("worktree_state_excluded") is not True
    ):
        blockers.append("source manifest is not the immutable held-out-v2 snapshot")
    observed_manifest_digest = manifest.get("manifest_digest")
    if not _is_digest(
        observed_manifest_digest
    ) or observed_manifest_digest != _sha256_bytes(
        _canonical_bytes(_without_key(manifest, "manifest_digest"))
    ):
        blockers.append("source manifest digest mismatch")
    repos = manifest.get("repos")
    if not isinstance(repos, list) or len(repos) != 4:
        return blockers + ["source manifest must contain exactly four repositories"]

    repo_ids: set[str] = set()
    source_roots: set[str] = set()
    for repo in repos:
        if not isinstance(repo, dict):
            blockers.append("source manifest contains a non-object repository")
            continue
        repo_id = repo.get("repo_id")
        if set(repo) != REPO_MANIFEST_FIELDS:
            blockers.append(f"repository {repo_id!r} violates its closed v2 field set")
        source_root = repo.get("source_root")
        if not isinstance(repo_id, str) or not repo_id:
            blockers.append("source manifest contains an invalid repository id")
            continue
        if repo_id in repo_ids:
            blockers.append(f"source manifest duplicates repository {repo_id}")
        repo_ids.add(repo_id)
        if not isinstance(source_root, str) or not source_root:
            blockers.append(f"repository {repo_id} source root is invalid")
        else:
            pure_root = pathlib.PurePosixPath(source_root)
            if pure_root.is_absolute() or ".." in pure_root.parts:
                blockers.append(
                    f"repository {repo_id} source root escapes its checkout"
                )
            if source_root in source_roots:
                blockers.append(f"repository {repo_id} source root is duplicated")
            source_roots.add(source_root)
        tree = repo.get("git_tree")
        if not isinstance(tree, str) or re.fullmatch(r"[0-9a-f]{40}", tree) is None:
            blockers.append(f"repository {repo_id} git tree is malformed")
        elif repo.get("source_revision") != f"git:{V2_SOURCE_COMMIT}:tree:{tree}":
            blockers.append(f"repository {repo_id} revision is not commit/tree bound")
        if (
            repo.get("primary_language") not in {"rust", "python", "typescript"}
            or repo.get("repo_size_band") not in SIZE_BAND_DEFINITION
            or repo.get("size_band_definition") != SIZE_BAND_DEFINITION
        ):
            blockers.append(f"repository {repo_id} language/size contract is malformed")
        files = repo.get("files")
        if not isinstance(files, list) or not files:
            blockers.append(f"repository {repo_id} file manifest is absent")
            continue
        if repo.get("file_set_digest") != _sha256_bytes(_canonical_bytes(files)):
            blockers.append(f"repository {repo_id} file-set digest mismatch")
        paths: set[str] = set()
        source_file_count = 0
        source_line_count = 0
        for entry in files:
            if not isinstance(entry, dict) or set(entry) != FILE_MANIFEST_FIELDS:
                blockers.append(f"repository {repo_id} has a malformed file entry")
                continue
            path = entry.get("path")
            pure = pathlib.PurePosixPath(path) if isinstance(path, str) else None
            if (
                pure is None
                or not path
                or pure.is_absolute()
                or ".." in pure.parts
                or pure.as_posix() != path
                or path in paths
            ):
                blockers.append(f"repository {repo_id} has a non-canonical file path")
            elif isinstance(path, str):
                paths.add(path)
            if entry.get("role") not in {"source", "dependency_manifest"}:
                blockers.append(f"repository {repo_id} has an unsupported file role")
            if (
                not isinstance(entry.get("bytes"), int)
                or isinstance(entry.get("bytes"), bool)
                or entry["bytes"] < 0
                or not isinstance(entry.get("lines"), int)
                or isinstance(entry.get("lines"), bool)
                or entry["lines"] < 0
                or not _is_digest(entry.get("sha256"))
            ):
                blockers.append(f"repository {repo_id} has malformed file metadata")
                continue
            if entry.get("role") == "source":
                source_file_count += 1
                source_line_count += entry["lines"]
        if (
            repo.get("searched_file_count") != len(files)
            or repo.get("source_file_count") != source_file_count
            or repo.get("source_line_count") != source_line_count
            or repo.get("repo_size_band") != _size_band(source_line_count)
        ):
            blockers.append(f"repository {repo_id} aggregate counts mismatch")
    return blockers


def _p95(values: list[float]) -> float:
    ordered = sorted(values)
    return ordered[max(0, math.ceil(0.95 * len(ordered)) - 1)]


def _exact_two_sided_binomial_p(k: int, n: int) -> float:
    """Two-sided exact sign-test p-value under p=0.5."""
    if n == 0:
        return 1.0
    observed = math.comb(n, k)
    numerator = sum(
        math.comb(n, i) for i in range(n + 1) if math.comb(n, i) <= observed
    )
    return min(1.0, numerator / (2**n))


def _not_proven(blockers: list[str], **extra: Any) -> dict[str, Any]:
    return {
        "schema": REPORT_SCHEMA,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "status": "NOT_PROVEN",
        "claimable": False,
        "blockers": sorted(set(blockers)),
        **extra,
    }


def _validate_self_digest(value: dict[str, Any], label: str) -> list[str]:
    observed = value.get("self_digest")
    if not _is_digest(observed):
        return [f"{label} self_digest is absent or malformed"]
    if observed != _self_digest(value):
        return [f"{label} self_digest mismatch"]
    return []


def _validate_spec(spec: dict[str, Any]) -> list[str]:
    blockers: list[str] = []
    if spec.get("schema") != SPEC_SCHEMA or spec.get("version") != 2:
        blockers.append(
            "metric spec is not the ratified v2 schema; legacy v1 evidence remains historical"
        )
    blockers += _validate_self_digest(spec, "metric spec")

    ratification = spec.get("ratification")
    if not isinstance(ratification, dict):
        blockers.append("metric spec ratification is absent")
    else:
        if ratification.get("status") != "ratified":
            blockers.append("metric spec is not ratified")
        if ratification.get("outcome_blind") is not True:
            blockers.append("metric spec ratification is not outcome-blind")
        if not _is_digest(ratification.get("authority_receipt_digest")):
            blockers.append("metric spec lacks an authority receipt digest")
        if ratification.get("unratified_fields") != []:
            blockers.append("metric spec still has unratified fields")

    corpus = spec.get("corpus")
    if not isinstance(corpus, dict):
        blockers.append("metric spec corpus contract is absent")
    else:
        for field in (
            "minimum_tasks",
            "minimum_languages",
            "minimum_repo_size_bands",
            "minimum_localizable",
            "minimum_unlocalizable",
        ):
            value = corpus.get(field)
            if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
                blockers.append(f"metric spec corpus field {field} is invalid")

    thresholds = spec.get("thresholds")
    if not isinstance(thresholds, dict):
        blockers.append("metric spec thresholds are absent")
    else:
        for field in (
            "top5_anchor_recall_min",
            "abstention_recall_min",
            "wrong_ground_action_rate_max",
            "regression_significance_alpha",
        ):
            value = thresholds.get(field)
            if not isinstance(value, (int, float)) or isinstance(value, bool):
                blockers.append(f"metric spec threshold {field} is invalid")
            elif not 0 <= float(value) <= 1:
                blockers.append(f"metric spec threshold {field} is outside [0,1]")

    latency = spec.get("latency_slo_ms")
    if not isinstance(latency, dict):
        blockers.append("metric spec latency SLOs are absent")
    else:
        for verb in ("north_p95", "seek_p95"):
            value = latency.get(verb)
            if (
                not isinstance(value, (int, float))
                or isinstance(value, bool)
                or value <= 0
                or not math.isfinite(float(value))
            ):
                blockers.append(f"{verb} latency SLO is not ratified")

    integrity = spec.get("measurement_integrity")
    required_integrity = {
        "require_error_free_runner_metadata": True,
        "reject_error_fallback_measurements": True,
        "include_fresh_session_overhead": True,
        "require_result_self_digest": True,
        "require_exact_lane_binding": True,
        "require_baseline_ratification_receipt": True,
        "require_sealed_run_ledger": True,
        "same_revision_rerun_policy": "one_sealed_run_only_no_rerun_until_pass",
    }
    if not isinstance(integrity, dict):
        blockers.append("metric spec measurement-integrity contract is absent")
    else:
        for field, expected in required_integrity.items():
            if integrity.get(field) != expected:
                blockers.append(f"measurement integrity field {field} is not ratified")
        floor = integrity.get("minimum_executed_latency_ms_exclusive")
        if (
            not isinstance(floor, (int, float))
            or isinstance(floor, bool)
            or floor < 0
            or not math.isfinite(float(floor))
        ):
            blockers.append("minimum executed latency floor is not ratified")

    calibration = spec.get("calibration")
    if not isinstance(calibration, dict):
        blockers.append("metric spec calibration gate is absent")
    else:
        integer_fields = (
            "minimum_calibration_sample_size",
            "minimum_authorized_action_count",
        )
        for field in integer_fields:
            value = calibration.get(field)
            if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
                blockers.append(f"calibration field {field} is invalid or vacuous")
        ratio_fields = (
            "minimum_calibration_precision",
            "minimum_calibration_coverage",
            "minimum_calibrated_task_fraction",
        )
        for field in ratio_fields:
            value = calibration.get(field)
            if (
                not isinstance(value, (int, float))
                or isinstance(value, bool)
                or not 0 < float(value) <= 1
            ):
                blockers.append(f"calibration field {field} is outside (0,1]")
    return blockers


def _validate_public_and_corpus(
    public: dict[str, Any], corpus: dict[str, Any], spec: dict[str, Any]
) -> list[str]:
    blockers: list[str] = []
    if public.get("schema") == HISTORICAL_PUBLIC_SCHEMA:
        blockers.append(
            "public held-out-v1 evidence is historical, not formal G6 evidence"
        )
    elif public.get("schema") != PUBLIC_SCHEMA or public.get("version") != 2:
        blockers.append("public corpus is not the held-out-v2 schema")
    if corpus.get("schema") == HISTORICAL_CASE_SCHEMA:
        blockers.append(
            "sealed held-out-v1 evidence is historical, not formal G6 evidence"
        )
    elif corpus.get("schema") != CASE_SCHEMA or corpus.get("version") != 2:
        blockers.append("held-out corpus is not the held-out-v2 schema")
    if set(public) != PUBLIC_FIELDS:
        blockers.append("public corpus violates its closed v2 field set")
    if set(corpus) != SEALED_FIELDS:
        blockers.append("sealed corpus violates its closed v2 field set")
    if public.get("blinded") is not True:
        blockers.append("public corpus is not marked blinded")
    if (
        corpus.get("blinded") is not True
        or corpus.get("adjudication_sealed_at") != V2_SEALED_AT
    ):
        blockers.append(
            "held-out corpus is not sealed as a blinded adjudication artifact"
        )
    if (
        public.get("author_review_status") != V2_AUTHOR_STATUS
        or corpus.get("author_review_status") != V2_AUTHOR_STATUS
    ):
        blockers.append("public/sealed v2 author-review status differs")
    blockers += _validate_self_digest(public, "public corpus")
    blockers += _validate_self_digest(corpus, "sealed corpus")

    public_manifest = public.get("source_manifest")
    sealed_manifest = corpus.get("source_manifest")
    if not isinstance(public_manifest, dict) or not isinstance(sealed_manifest, dict):
        blockers.append("public or sealed source manifest is absent")
        return blockers
    if public_manifest != sealed_manifest:
        blockers.append("public and sealed source manifests differ")
    manifest = sealed_manifest
    blockers += _validate_source_manifest_contract(manifest)
    observed_manifest_digest = manifest.get("manifest_digest")

    runner_contract = public.get("runner_contract")
    if not isinstance(runner_contract, dict):
        blockers.append("public runner contract is absent")
    else:
        if set(runner_contract) != RUNNER_CONTRACT_FIELDS:
            blockers.append("public runner contract violates its closed v2 field set")
        expected_runner_contract = {
            "read_only_artifact": "public/queries.json",
            "forbidden_artifact": "operator-only/corpus.json",
            "result_coverage": "emit exactly one measurement for every task_id",
            "source_checkout": V2_SOURCE_COMMIT,
            "labels_exposed": False,
            "independent_review_status": "NOT_RUN",
        }
        if runner_contract != expected_runner_contract:
            blockers.append("public runner contract differs from held-out-v2")

    public_tasks = public.get("tasks")
    sealed_tasks = corpus.get("tasks")
    if not isinstance(public_tasks, list) or not isinstance(sealed_tasks, list):
        blockers.append("public or sealed corpus tasks are absent")
        return blockers
    if public.get("task_count") != 220 or len(public_tasks) != 220:
        blockers.append("public corpus task_count mismatch")
    minimum = spec.get("corpus", {}).get("minimum_tasks", 200)
    if not isinstance(minimum, int) or isinstance(minimum, bool):
        minimum = 200
    if len(sealed_tasks) < minimum:
        blockers.append(f"corpus has {len(sealed_tasks)} tasks; minimum is {minimum}")
    if len(public_tasks) != len(sealed_tasks):
        blockers.append("public and sealed task counts differ")

    projected: list[dict[str, Any]] = []
    for task in sealed_tasks:
        if isinstance(task, dict):
            projected.append({field: task.get(field) for field in PUBLIC_TASK_FIELDS})
        else:
            projected.append({})
    if public_tasks != projected:
        blockers.append("public corpus is not the exact label-blind sealed projection")
    for task in public_tasks:
        if not isinstance(task, dict) or set(task) != PUBLIC_TASK_FIELDS:
            blockers.append(
                "public corpus contains missing, unknown, or label-bearing fields"
            )
            break

    expected_corpus_digest = _sha256_bytes(
        _canonical_bytes(
            {
                "source_manifest_digest": observed_manifest_digest,
                "tasks": public_tasks,
            }
        )
    )
    observed_corpus_digest = corpus.get("corpus_digest")
    if (
        not _is_digest(observed_corpus_digest)
        or observed_corpus_digest != expected_corpus_digest
        or public.get("corpus_digest") != expected_corpus_digest
    ):
        blockers.append("public/sealed corpus digest mismatch")
    expected_corpus_id = (
        "m1nd10-g6-held-out-v2-" + expected_corpus_digest.removeprefix("sha256:")[:16]
    )
    if (
        corpus.get("corpus_id") != expected_corpus_id
        or public.get("corpus_id") != expected_corpus_id
    ):
        blockers.append("public/sealed corpus id is not derived from the corpus digest")

    ids: list[Any] = []
    languages: set[Any] = set()
    size_bands: set[Any] = set()
    localizable = 0
    unlocalizable = 0
    repo_revisions: dict[str, str] = {}
    repos = {
        repo.get("repo_id"): repo
        for repo in manifest.get("repos", [])
        if isinstance(repo, dict)
    }
    for task in sealed_tasks:
        if not isinstance(task, dict):
            blockers.append("sealed corpus contains a non-object task")
            continue
        required = (
            "task_id",
            "repo_id",
            "repo_revision",
            "language",
            "repo_size_band",
            "query",
        )
        if any(not task.get(field) for field in required):
            blockers.append(
                f"task {task.get('task_id', '<missing>')} lacks immutable identity fields"
            )
        ids.append(task.get("task_id"))
        languages.add(task.get("language"))
        size_bands.add(task.get("repo_size_band"))
        repo_id = task.get("repo_id")
        repo_revision = task.get("repo_revision")
        if isinstance(repo_id, str) and isinstance(repo_revision, str):
            previous = repo_revisions.setdefault(repo_id, repo_revision)
            if previous != repo_revision:
                blockers.append(f"repo {repo_id} has multiple corpus revisions")
            repo = repos.get(repo_id)
            if (
                repo is None
                or repo_revision != repo.get("source_revision")
                or task.get("language") != repo.get("primary_language")
                or task.get("repo_size_band") != repo.get("repo_size_band")
            ):
                blockers.append(
                    f"task {task.get('task_id', '<missing>')} has a foreign manifest binding"
                )
        if (
            not isinstance(task.get("task_id"), str)
            or re.fullmatch(r"g6-(?:mcp|core|py|ui)-[0-9a-f]{16}", task["task_id"])
            is None
            or not isinstance(task.get("query"), str)
            or len(task["query"]) < 55
            or not task["query"].endswith("?")
        ):
            blockers.append(
                f"task {task.get('task_id', '<missing>')} violates the v2 public identity surface"
            )
        is_localizable = task.get("localizable")
        anchors = task.get("accepted_anchor_ids")
        if is_localizable is True and isinstance(anchors, list) and anchors:
            localizable += 1
        elif is_localizable is False and anchors == []:
            unlocalizable += 1
        else:
            blockers.append(
                f"task {task.get('task_id', '<missing>')} has inconsistent localizable/anchor labels"
            )
    if None in ids or len(ids) != len(set(ids)):
        blockers.append("task ids are missing or duplicated")
    corpus_spec = spec.get("corpus")
    if not isinstance(corpus_spec, dict):
        corpus_spec = {}
    if len({value for value in languages if value}) < corpus_spec.get(
        "minimum_languages", 2
    ):
        blockers.append("corpus does not cover the required language diversity")
    if len({value for value in size_bands if value}) < corpus_spec.get(
        "minimum_repo_size_bands", 2
    ):
        blockers.append("corpus does not cover the required repository-size diversity")
    if localizable < corpus_spec.get("minimum_localizable", 1):
        blockers.append("corpus lacks enough localizable tasks")
    if unlocalizable < corpus_spec.get("minimum_unlocalizable", 1):
        blockers.append("corpus lacks enough unlocalizable tasks")
    expected_counts = {
        "total": len(sealed_tasks),
        "localizable": localizable,
        "unlocalizable": unlocalizable,
        "by_language": dict(
            sorted(
                Counter(task.get("language") for task in sealed_tasks).items(),
                key=lambda item: str(item[0]),
            )
        ),
        "by_repo_size_band": dict(
            sorted(
                Counter(task.get("repo_size_band") for task in sealed_tasks).items(),
                key=lambda item: str(item[0]),
            )
        ),
        "by_repo": dict(
            sorted(
                Counter(task.get("repo_id") for task in sealed_tasks).items(),
                key=lambda item: str(item[0]),
            )
        ),
    }
    if corpus.get("counts") != expected_counts:
        blockers.append("sealed corpus counts do not match its tasks")
    if not isinstance(corpus.get("methodology"), dict) or not corpus["methodology"]:
        blockers.append("sealed corpus methodology is absent")
    return blockers


def _expected_source_revision(corpus: dict[str, Any]) -> str | None:
    manifest = corpus.get("source_manifest")
    if not isinstance(manifest, dict):
        return None
    value = manifest.get("source_commit") or manifest.get("snapshot_digest")
    return value if isinstance(value, str) and value else None


def _result_binding(
    artifact: dict[str, Any],
    public: dict[str, Any],
    corpus: dict[str, Any],
    metric_spec_file_digest: str,
    runner_file_digest: str,
    expected_binary_digest: str,
) -> dict[str, Any]:
    manifest = (
        corpus.get("source_manifest")
        if isinstance(corpus.get("source_manifest"), dict)
        else {}
    )
    return {
        "corpus_id": corpus.get("corpus_id"),
        "corpus_digest": corpus.get("corpus_digest"),
        "public_corpus_self_digest": public.get("self_digest"),
        "sealed_corpus_self_digest": corpus.get("self_digest"),
        "source_manifest_digest": manifest.get("manifest_digest"),
        "source_revision": _expected_source_revision(corpus),
        "metric_spec_digest": metric_spec_file_digest,
        "runner_digest": runner_file_digest,
        "binary_digest": expected_binary_digest,
    }


def _optional_sha256(value: Any) -> bool:
    return (
        isinstance(value, str)
        and re.fullmatch(r"(?:sha256:)?[0-9a-f]{64}", value) is not None
    )


def _positive_integer(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def _canonical_absolute_posix(value: Any) -> pathlib.PurePosixPath | None:
    if not isinstance(value, str) or not value or "\\" in value or "\x00" in value:
        return None
    path = pathlib.PurePosixPath(value)
    if (
        not path.is_absolute()
        or path.as_posix() != value
        or value == "/"
        or any(part in {".", ".."} for part in path.parts)
    ):
        return None
    return path


def _path_is_within(
    child: pathlib.PurePosixPath, parent: pathlib.PurePosixPath
) -> bool:
    return child == parent or parent in child.parents


def _validate_path_topology(
    value: Any,
) -> tuple[list[str], bool, dict[str, pathlib.PurePosixPath]]:
    errors: list[str] = []
    if not isinstance(value, dict) or set(value) != PATH_TOPOLOGY_PROOF_FIELDS:
        return ["formal path topology proof is malformed"], False, {}
    paths = value.get("paths")
    required = {"source_root", "runtime_dir", "registry_dir", "output"}
    allowed = required | {"checkpoint"}
    if (
        not isinstance(paths, dict)
        or not required.issubset(paths)
        or not set(paths).issubset(allowed)
    ):
        return ["formal path topology paths are incomplete or open"], False, {}
    parsed: dict[str, pathlib.PurePosixPath] = {}
    for name, raw_path in paths.items():
        path = _canonical_absolute_posix(raw_path)
        if path is None:
            errors.append(
                f"formal path topology {name} is not canonical absolute POSIX"
            )
        else:
            parsed[name] = path
    disjoint = len(parsed) == len(paths)
    names = sorted(parsed)
    for index, left_name in enumerate(names):
        for right_name in names[index + 1 :]:
            left = parsed[left_name]
            right = parsed[right_name]
            if _path_is_within(left, right) or _path_is_within(right, left):
                errors.append(
                    f"formal path topology overlaps {left_name} and {right_name}"
                )
                disjoint = False
    if value.get("disjoint") is not disjoint:
        errors.append(
            "formal path topology disjoint summary is not independently derived"
        )
    proven = bool(
        value.get("absolute") is True
        and value.get("fresh_mutable_roots") is True
        and value.get("symlink_free_path_components") is True
        and value.get("disjoint") is True
        and disjoint
        and not errors
    )
    return errors, proven, parsed


def _manifest_repo_bindings(
    source_manifest: Any,
) -> tuple[dict[str, dict[str, Any]], list[str]]:
    errors: list[str] = []
    if not isinstance(source_manifest, dict):
        return {}, ["formal source manifest is absent"]
    repos = source_manifest.get("repos")
    if not isinstance(repos, list) or not repos:
        return {}, ["formal source manifest repository set is absent"]
    bindings: dict[str, dict[str, Any]] = {}
    for repo in repos:
        if not isinstance(repo, dict):
            errors.append("formal source manifest has a malformed repository")
            continue
        repo_id = repo.get("repo_id")
        if not isinstance(repo_id, str) or not repo_id or repo_id in bindings:
            errors.append(
                "formal source manifest repository ids are missing or duplicated"
            )
            continue
        bindings[repo_id] = repo
    return bindings, errors


def _validate_source_verification(
    value: Any, expected_repos: dict[str, dict[str, Any]]
) -> tuple[list[str], bool]:
    if not isinstance(value, dict) or set(value) != SOURCE_VERIFICATION_FIELDS:
        return ["source verification violates its closed JSON field set"], False
    expected_files = sum(len(repo.get("files", [])) for repo in expected_repos.values())
    expected_bytes = sum(
        entry.get("bytes", -1)
        for repo in expected_repos.values()
        for entry in repo.get("files", [])
        if isinstance(entry, dict)
    )
    expected_lines = sum(
        entry.get("lines", -1)
        for repo in expected_repos.values()
        for entry in repo.get("files", [])
        if isinstance(entry, dict)
    )
    repo_roots = value.get("repo_roots")
    roots_valid = bool(
        isinstance(repo_roots, dict)
        and set(repo_roots) == set(expected_repos)
        and all(
            _canonical_absolute_posix(path) is not None for path in repo_roots.values()
        )
    )
    proven = bool(
        value.get("checked_files") == expected_files
        and value.get("missing_files") == 0
        and value.get("digest_mismatches") == 0
        and value.get("extra_files") == 0
        and value.get("checked_bytes") == expected_bytes
        and value.get("checked_lines") == expected_lines
        and value.get("exact_live_file_set") is True
        and value.get("symlinks_rejected") is True
        and value.get("isolated_snapshot_required") is True
        and value.get("git_objects_used_as_live_root") is False
        and roots_valid
    )
    return ([] if proven else ["source verification is incomplete or foreign"]), proven


def _validate_formal_run_proof(
    metadata: dict[str, Any],
    expected_binary_digest: str,
    source_manifest: Any,
) -> tuple[list[str], bool]:
    """Re-derive score eligibility from raw proof rows inside the scorer.

    The runner's declarations are comparison values only. This function does not
    import or call runner validation code and never accepts a summary flag as the
    source of formal completeness.
    """

    errors: list[str] = []
    if set(metadata) != RUN_METADATA_FIELDS:
        errors.append("run metadata violates its closed JSON field set")

    formal = metadata.get("formal_preflights")
    if not isinstance(formal, dict) or set(formal) != FORMAL_PREFLIGHT_FIELDS:
        return errors + ["formal_preflights violates its closed JSON field set"], False
    missing = formal.get("missing")
    if not isinstance(missing, list) or any(
        not isinstance(item, str) or not item for item in missing
    ):
        errors.append("formal_preflights missing list is malformed")
        missing = []

    blind = formal.get("authority_blind_boundary")
    blind_coherent = bool(
        isinstance(blind, dict)
        and set(blind) == {"kind", "proven"}
        and blind.get("kind") == metadata.get("authority_blind_boundary_kind")
        and blind.get("proven") is metadata.get("authority_blind_boundary_proven")
    )
    if not blind_coherent:
        errors.append("formal authority blind-boundary proof is incoherent")
        blind = {}

    path_errors, path_proven, paths = _validate_path_topology(
        formal.get("path_topology")
    )
    errors.extend(path_errors)
    expected_repos, manifest_errors = _manifest_repo_bindings(source_manifest)
    errors.extend(manifest_errors)

    for field_name, prefixed in (
        ("authority_assembly_digest", False),
        ("authority_provider_executable_digest", True),
        ("authority_owner_security_config_digest", True),
    ):
        value = metadata.get(field_name)
        pattern = r"sha256:[0-9a-f]{64}" if prefixed else r"[0-9a-f]{64}"
        if not isinstance(value, str) or re.fullmatch(pattern, value) is None:
            errors.append(f"run_metadata invalid {field_name}")
    if not _positive_integer(metadata.get("authority_key_registry_epoch")):
        errors.append("run_metadata authority key registry epoch is invalid")
    if not isinstance(
        metadata.get("authority_receipt_key_id"), str
    ) or not metadata.get("authority_receipt_key_id"):
        errors.append("run_metadata authority receipt key id is invalid")

    topologies = metadata.get("owner_topology")
    cleanups = metadata.get("owner_cleanup")
    ingests = metadata.get("governed_graph_ingest")
    if not isinstance(topologies, list):
        errors.append("owner_topology is malformed")
        topologies = []
    if not isinstance(cleanups, list):
        errors.append("owner_cleanup is malformed")
        cleanups = []
    if not isinstance(ingests, list):
        errors.append("governed_graph_ingest is malformed")
        ingests = []

    cleanup_by_repo: dict[str, dict[str, Any]] = {}
    cleanup_proven = bool(cleanups)
    for cleanup in cleanups:
        if not isinstance(cleanup, dict) or set(cleanup) != OWNER_CLEANUP_FIELDS:
            errors.append("owner cleanup proof violates its closed JSON field set")
            cleanup_proven = False
            continue
        repo_id = cleanup.get("repo_id")
        if not isinstance(repo_id, str) or not repo_id or repo_id in cleanup_by_repo:
            errors.append("owner cleanup repository ids are missing or duplicated")
            cleanup_proven = False
            continue
        cleanup_by_repo[repo_id] = cleanup
        proven = bool(
            cleanup.get("same_session_for_owner_lifetime") is True
            and cleanup.get("session_delete_proven") is True
            and cleanup.get("process_group_terminated") is True
            and cleanup.get("cleanup_complete") is True
        )
        cleanup_proven = cleanup_proven and proven
        if not proven:
            errors.append(f"{repo_id}: cleanup is incomplete")

    topology_by_repo: dict[str, dict[str, Any]] = {}
    topology_bindings_proven = bool(topologies)
    for topology in topologies:
        if not isinstance(topology, dict) or set(topology) != OWNER_TOPOLOGY_FIELDS:
            errors.append("owner topology violates its closed JSON field set")
            topology_bindings_proven = False
            continue
        repo_id = topology.get("repo_id")
        if not isinstance(repo_id, str) or not repo_id or repo_id in topology_by_repo:
            errors.append("owner topology repository ids are missing or duplicated")
            topology_bindings_proven = False
            continue
        topology_by_repo[repo_id] = topology
        expected = expected_repos.get(repo_id)
        readiness = topology.get("readiness")
        nested_cleanup = topology.get("cleanup")
        readiness_valid = bool(
            isinstance(readiness, dict)
            and set(readiness) == OWNER_READINESS_FIELDS
            and _positive_integer(readiness.get("pid"))
            and _positive_integer(readiness.get("started_at_ms"))
            and _optional_sha256(readiness.get("registry_entry_digest"))
            and _optional_sha256(readiness.get("manifest_digest"))
            and readiness.get("binary_digest") == expected_binary_digest
            and readiness.get("token_captured_once") is True
            and readiness.get("owner_binding_proven") is True
        )
        port = topology.get("port")
        source_path = _canonical_absolute_posix(topology.get("source_root"))
        runtime_path = _canonical_absolute_posix(topology.get("runtime_dir"))
        registry_path = _canonical_absolute_posix(topology.get("registry_dir"))
        paths_bound = bool(
            source_path is not None
            and runtime_path is not None
            and registry_path is not None
            and paths.get("source_root") is not None
            and paths.get("runtime_dir") is not None
            and paths.get("registry_dir") is not None
            and _path_is_within(source_path, paths["source_root"])
            and _path_is_within(runtime_path, paths["runtime_dir"])
            and _path_is_within(registry_path, paths["registry_dir"])
        )
        binding = bool(
            expected is not None
            and topology.get("source_revision") == expected.get("source_revision")
            and topology.get("file_set_digest") == expected.get("file_set_digest")
            and isinstance(topology.get("owner_id"), str)
            and bool(topology.get("owner_id"))
            and isinstance(topology.get("instance_id"), str)
            and bool(topology.get("instance_id"))
            and isinstance(topology.get("mcp_session_id"), str)
            and bool(topology.get("mcp_session_id"))
            and topology.get("process_isolated") is True
            and topology.get("mcp_session_isolated") is True
            and isinstance(port, int)
            and not isinstance(port, bool)
            and 1 <= port <= 65_535
            and port != INSTALLED_OWNER_PORT
            and readiness_valid
            and isinstance(nested_cleanup, dict)
            and nested_cleanup == cleanup_by_repo.get(repo_id)
            and paths_bound
        )
        topology_bindings_proven = topology_bindings_proven and binding
        if not binding:
            errors.append(
                f"{repo_id}: owner readiness binding is incomplete or foreign"
            )

    ingest_by_repo: dict[str, dict[str, Any]] = {}
    authority_receipts_proven = bool(ingests)
    for ingest in ingests:
        if not isinstance(ingest, dict) or set(ingest) != GOVERNED_INGEST_FIELDS:
            errors.append("governed ingest violates its closed JSON field set")
            authority_receipts_proven = False
            continue
        repo_id = ingest.get("repo_id")
        if not isinstance(repo_id, str) or not repo_id or repo_id in ingest_by_repo:
            errors.append("governed ingest repository ids are missing or duplicated")
            authority_receipts_proven = False
            continue
        ingest_by_repo[repo_id] = ingest
        expected = expected_repos.get(repo_id)
        topology = topology_by_repo.get(repo_id)
        receipt = ingest.get("authority_receipt")
        receipt_shape = bool(
            isinstance(receipt, dict) and set(receipt) == AUTHORITY_RECEIPT_PROOF_FIELDS
        )
        if not receipt_shape:
            errors.append(f"{repo_id}: authority receipt proof is malformed")
            authority_receipts_proven = False
            continue
        receipt_proven = bool(
            receipt.get("production_authority_receipt_proven") is True
            and receipt.get("control_verified_ed25519") is True
            and receipt.get("receipt_core_digest_verified") is True
            and receipt.get("assembly_digest_verified") is True
            and receipt.get("signature_verified") is True
            and receipt.get("clock_verified") is True
            and receipt.get("key_lifecycle_verified") is True
            and receipt.get("receipt_signer_metadata_production") is True
            and receipt.get("key_registry_epoch")
            == metadata.get("authority_key_registry_epoch")
            and receipt.get("key_id") == metadata.get("authority_receipt_key_id")
            and receipt.get("algorithm") == "ED25519"
            and _positive_integer(receipt.get("checked_at_ms"))
            and _optional_sha256(receipt.get("receipt_digest"))
            and isinstance(receipt.get("issuer"), str)
            and bool(receipt.get("issuer"))
        )
        binding = bool(
            expected is not None
            and topology is not None
            and ingest.get("owner_id") == topology.get("owner_id")
            and ingest.get("mcp_session_id") == topology.get("mcp_session_id")
            and ingest.get("source_revision") == expected.get("source_revision")
            and ingest.get("file_set_digest") == expected.get("file_set_digest")
            and ingest.get("authorization_lease_bound") is True
            and ingest.get("production_authority_receipt_proven") is True
            and ingest.get("reconciliation_state") == "RECONCILED"
            and receipt_proven
        )
        authority_receipts_proven = authority_receipts_proven and binding
        if not binding:
            errors.append(
                f"{repo_id}: production authority proof is incomplete or foreign"
            )

    expected_repo_ids = set(expected_repos)
    same_repo_set = bool(expected_repo_ids) and (
        set(topology_by_repo)
        == set(cleanup_by_repo)
        == set(ingest_by_repo)
        == expected_repo_ids
        and len(topologies) == len(cleanups) == len(ingests) == len(expected_repo_ids)
    )
    if not same_repo_set:
        errors.append(
            "owner topology/cleanup/ingest repository sets differ from the corpus"
        )
    if metadata.get("governed_setup_mutations_executed") != len(expected_repo_ids):
        errors.append("governed setup mutation count mismatch")

    source_errors, source_live = _validate_source_verification(
        metadata.get("source_verification"), expected_repos
    )
    errors.extend(source_errors)
    post_source = metadata.get("post_ingest_source_verification")
    source_post = bool(
        source_live and post_source == metadata.get("source_verification")
    )
    if not source_post:
        errors.append(
            "post-ingest source verification differs from the sealed pre-ingest proof"
        )

    derived_complete = bool(
        metadata.get("authority_mode") == "formal"
        and metadata.get("authority_provider_kind") == "production"
        and metadata.get("authority_provider_claimed_production_assembly") is True
        and metadata.get("authority_assembly_digest_verified") is True
        and metadata.get("authority_blind_boundary_proven") is True
        and metadata.get("labels_read") is False
        and metadata.get("benchmark_task_actions_executed") == 0
        and formal.get("delivery") == "delivery-2-hardened-runner"
        and blind_coherent
        and blind.get("proven") is True
        and path_proven
        and source_live
        and source_post
        and same_repo_set
        and topology_bindings_proven
        and cleanup_proven
        and authority_receipts_proven
    )
    lifecycle_proven = bool(
        cleanup_proven and topology_bindings_proven and same_repo_set
    )
    summary_expectations = (
        (
            formal.get("same_session_readiness_ingest_measurement_delete"),
            lifecycle_proven,
            "formal same-session lifecycle summary is not independently derived",
        ),
        (
            formal.get("process_group_cleanup"),
            cleanup_proven,
            "formal process-group cleanup summary is not independently derived",
        ),
        (
            formal.get("source_live_identity"),
            source_live,
            "formal source identity summary is not independently derived",
        ),
        (
            formal.get("source_post_ingest_identity"),
            source_post,
            "formal post-ingest source summary is not independently derived",
        ),
        (
            formal.get("owner_readiness_bindings_proven"),
            topology_bindings_proven,
            "formal owner readiness summary is not independently derived",
        ),
        (
            formal.get("authority_receipts_proven"),
            authority_receipts_proven,
            "formal authority receipt summary is not independently derived",
        ),
        (
            metadata.get("production_authority_assembly_proven"),
            authority_receipts_proven,
            "authority assembly proof summary differs from receipt evidence",
        ),
    )
    for declared, derived, message in summary_expectations:
        if declared is not derived:
            errors.append(message)

    if (
        formal.get("complete") is not derived_complete
        or formal.get("status") != ("PROVEN" if derived_complete else PROOF_NOT_PROVEN)
        or (derived_complete and missing)
        or (not derived_complete and not missing)
    ):
        errors.append("formal preflight summary is not independently derived")
    if metadata.get("score_eligible") is not derived_complete:
        errors.append("declared score eligibility differs from scorer-derived proof")
    if metadata.get("diagnostic_only") is not (not derived_complete):
        errors.append(
            "declared diagnostic-only state differs from scorer-derived proof"
        )
    if metadata.get("proof_state") != (
        "PROVEN" if derived_complete else PROOF_NOT_PROVEN
    ):
        errors.append("declared proof state differs from scorer-derived proof")
    return errors, derived_complete


def _index_results(
    artifact: dict[str, Any],
    public: dict[str, Any],
    corpus: dict[str, Any],
    spec: dict[str, Any],
    label: str,
    metric_spec_file_digest: str,
    runner_file_digest: str,
    expected_binary_digest: str,
) -> tuple[dict[str, dict[str, Any]], list[str]]:
    blockers: list[str] = []
    if artifact.get("schema") != RESULT_SCHEMA:
        blockers.append(
            f"{label} result is not v2; legacy results remain historical and are not silently upgraded"
        )
    blockers += _validate_self_digest(artifact, f"{label} result")
    if artifact.get("lane") != label:
        blockers.append(f"{label} top-level lane binding is absent or wrong")
    if not isinstance(artifact.get("run_id"), str) or not artifact.get("run_id"):
        blockers.append(f"{label} run_id is absent")
    if not isinstance(artifact.get("system_revision"), str) or not artifact.get(
        "system_revision"
    ):
        blockers.append(f"{label} exact system revision is absent")

    expected_binding = _result_binding(
        artifact,
        public,
        corpus,
        metric_spec_file_digest,
        runner_file_digest,
        expected_binary_digest,
    )
    for field, expected in expected_binding.items():
        if artifact.get(field) != expected:
            blockers.append(
                f"{label} {field} does not match the exact evidence binding"
            )
    for field in (
        "corpus_digest",
        "public_corpus_self_digest",
        "sealed_corpus_self_digest",
        "source_manifest_digest",
        "metric_spec_digest",
        "runner_digest",
        "binary_digest",
    ):
        if not _is_digest(artifact.get(field)):
            blockers.append(f"{label} {field} is absent or malformed")

    metadata = artifact.get("run_metadata")
    if not isinstance(metadata, dict):
        blockers.append(f"{label} lacks blinded runner metadata")
        metadata = {}
    else:
        proof_errors, formal_complete = _validate_formal_run_proof(
            metadata,
            expected_binary_digest,
            corpus.get("source_manifest"),
        )
        blockers.extend(f"{label} {error}" for error in proof_errors)
        if metadata.get("schema") != RUN_METADATA_SCHEMA:
            blockers.append(f"{label} runner metadata schema is missing or unsupported")
        if metadata.get("lane") != label:
            blockers.append(f"{label} runner metadata lane binding is absent or wrong")
        if metadata.get("transport") != "mcp-http-loopback":
            blockers.append(
                f"{label} runner transport is not the closed loopback MCP path"
            )
        if metadata.get("task_count") != len(corpus.get("tasks", [])):
            blockers.append(f"{label} runner task count differs from the corpus")
        for field in ("generated_at", "started_at"):
            if not isinstance(metadata.get(field), str) or not metadata[field].strip():
                blockers.append(f"{label} runner {field} is absent")
        if metadata.get("errors") != []:
            blockers.append(f"{label} contains runner/tool errors")
        if (
            metadata.get("actions_executed") != 0
            or metadata.get("benchmark_task_actions_executed") != 0
        ):
            blockers.append(f"{label} executed actions during the read-only benchmark")
        if (
            metadata.get("labels_read") is not False
            or metadata.get("unscored") is not True
        ):
            blockers.append(f"{label} does not prove blinded unscored execution")
        if not formal_complete:
            blockers.append(f"{label} scorer-derived formal proof is incomplete")
        if metadata.get("run_id") != artifact.get("run_id"):
            blockers.append(f"{label} metadata run_id mismatch")
        raw_counts = metadata.get("raw_runtime_verdict_counts")
        if not isinstance(raw_counts, dict) or raw_counts.get("error_fallback", 0) != 0:
            blockers.append(
                f"{label} contains or fails to exclude error-fallback measurements"
            )

    measurements = artifact.get("measurements")
    if not isinstance(measurements, list):
        return {}, blockers + [f"{label} measurements are absent"]
    ids = [row.get("task_id") for row in measurements if isinstance(row, dict)]
    if len(ids) != len(measurements):
        blockers.append(f"{label} contains a non-object measurement")
    if None in ids or len(ids) != len(set(ids)):
        blockers.append(f"{label} task ids are missing or duplicated")
    expected = {
        task.get("task_id")
        for task in corpus.get("tasks", [])
        if isinstance(task, dict)
    }
    observed = set(ids)
    if expected != observed or len(ids) != len(expected):
        blockers.append(
            f"{label} task coverage differs from corpus (missing={len(expected - observed)}, "
            f"extra={len(observed - expected)})"
        )

    indexed: dict[str, dict[str, Any]] = {}
    integrity = spec.get("measurement_integrity")
    if not isinstance(integrity, dict):
        integrity = {}
    latency_floor = integrity.get("minimum_executed_latency_ms_exclusive", 0.0)
    calibration_spec = spec.get("calibration")
    if not isinstance(calibration_spec, dict):
        calibration_spec = {}
    calibrated_count = 0
    act_count = 0
    verdict_counts: Counter[str] = Counter()
    calibration_receipts: set[str] = set()
    for row in measurements:
        if not isinstance(row, dict):
            continue
        task_id = row.get("task_id")
        if not isinstance(task_id, str) or not task_id:
            continue
        ranked = row.get("ranked_anchor_ids")
        if (
            not isinstance(ranked, list)
            or len(ranked) > 5
            or len(ranked) != len(set(ranked))
            or any(not isinstance(anchor, str) or not anchor for anchor in ranked)
        ):
            blockers.append(
                f"{label} task {task_id} has invalid/duplicate ranked anchors"
            )
        verdict = row.get("verdict")
        if verdict not in VALID_VERDICTS:
            blockers.append(f"{label} task {task_id} has an invalid verdict")
        else:
            verdict_counts[verdict] += 1
            act_count += verdict == "act"
        if row.get("seek_executed") is not True:
            blockers.append(f"{label} task {task_id} lacks executed seek evidence")
        if row.get("north_executed") is not True:
            blockers.append(f"{label} task {task_id} lacks executed north evidence")
        for verb in ("north_latency_ms", "seek_latency_ms"):
            value = row.get(verb)
            if (
                not isinstance(value, (int, float))
                or isinstance(value, bool)
                or value <= latency_floor
                or not math.isfinite(float(value))
            ):
                blockers.append(
                    f"{label} task {task_id} lacks a finite executed {verb}"
                )
        envelope = row.get("trust_envelope")
        if not isinstance(envelope, dict):
            blockers.append(
                f"{label} task {task_id} lacks a trust-envelope calibration stamp"
            )
        else:
            if envelope.get("calibrated") is True:
                calibrated_count += 1
            if envelope.get("verdict") != verdict:
                blockers.append(
                    f"{label} task {task_id} trust-envelope verdict mismatch"
                )
            receipt_digest = envelope.get("calibration_receipt_digest")
            if not _is_digest(receipt_digest):
                blockers.append(
                    f"{label} task {task_id} lacks a calibration receipt digest"
                )
            else:
                calibration_receipts.add(receipt_digest)
        indexed[task_id] = row

    metadata_counts = metadata.get("raw_runtime_verdict_counts")
    expected_counts = dict(sorted(verdict_counts.items()))
    if isinstance(metadata_counts, dict) and metadata_counts != expected_counts:
        blockers.append(f"{label} raw verdict counts do not match measurement rows")

    calibration = metadata.get("calibration")
    if not isinstance(calibration, dict):
        blockers.append(f"{label} calibration summary is absent")
        calibration = {}
    else:
        if calibration.get("schema") != CALIBRATION_SCHEMA:
            blockers.append(f"{label} calibration summary schema is unsupported")
        if calibration.get("status") != "armed":
            blockers.append(f"{label} calibration gate is not armed")
        if calibration.get("receipt_schema") != SEEK_CALIBRATION_RECEIPT_SCHEMA:
            blockers.append(f"{label} calibration receipt schema is unsupported")
        if calibration.get("signal") != SEEK_CALIBRATION_SIGNAL:
            blockers.append(f"{label} calibration signal is not the seek envelope")
        if calibration.get("calibrated_task_count") != calibrated_count:
            blockers.append(f"{label} calibrated-task count does not match rows")
        if calibration.get("authorized_action_count") != act_count:
            blockers.append(f"{label} authorized-action count does not match rows")
        receipt_digest = calibration.get("receipt_digest")
        if not _is_digest(receipt_digest):
            blockers.append(f"{label} calibration receipt digest is absent")
        elif calibration_receipts != {receipt_digest}:
            blockers.append(
                f"{label} per-task calibration receipt binding is inconsistent"
            )
        sample_size = calibration.get("sample_size")
        if (
            not isinstance(sample_size, int)
            or isinstance(sample_size, bool)
            or sample_size < calibration_spec.get("minimum_calibration_sample_size", 1)
        ):
            blockers.append(
                f"{label} calibration sample size is below the ratified minimum"
            )
        for field in ("tau", "target_alpha"):
            value = calibration.get(field)
            if (
                not isinstance(value, (int, float))
                or isinstance(value, bool)
                or not math.isfinite(float(value))
                or not 0 <= float(value) <= 1
            ):
                blockers.append(f"{label} calibration {field} is invalid")
        calibrated_at_ms = calibration.get("calibrated_at_ms")
        if (
            not isinstance(calibrated_at_ms, int)
            or isinstance(calibrated_at_ms, bool)
            or calibrated_at_ms <= 0
        ):
            blockers.append(f"{label} calibration timestamp is invalid")
        precision = calibration.get("measured_precision")
        if (
            not isinstance(precision, (int, float))
            or isinstance(precision, bool)
            or not math.isfinite(float(precision))
            or precision < calibration_spec.get("minimum_calibration_precision", 1.0)
        ):
            blockers.append(
                f"{label} calibration precision is below the ratified minimum"
            )
        coverage = calibration.get("coverage")
        if (
            not isinstance(coverage, (int, float))
            or isinstance(coverage, bool)
            or not math.isfinite(float(coverage))
            or coverage < calibration_spec.get("minimum_calibration_coverage", 1.0)
        ):
            blockers.append(
                f"{label} calibration coverage is below the ratified minimum"
            )

    calibrated_fraction = calibrated_count / len(measurements) if measurements else 0.0
    if calibrated_fraction < calibration_spec.get(
        "minimum_calibrated_task_fraction", 1.0
    ):
        blockers.append(
            f"{label} calibrated task fraction is below the ratified minimum"
        )
    if act_count < calibration_spec.get("minimum_authorized_action_count", 1):
        blockers.append(
            f"{label} authorized-action sample is empty or below the ratified minimum"
        )
    return indexed, blockers


def _validate_baseline_receipt(
    receipt: dict[str, Any],
    baseline: dict[str, Any],
    public: dict[str, Any],
    corpus: dict[str, Any],
    metric_spec_file_digest: str,
    runner_file_digest: str,
    baseline_binary_digest: str,
) -> list[str]:
    blockers: list[str] = []
    if receipt.get("schema") != BASELINE_RECEIPT_SCHEMA or receipt.get("version") != 1:
        blockers.append(
            "baseline-ratification receipt schema is missing or unsupported"
        )
    blockers += _validate_self_digest(receipt, "baseline-ratification receipt")
    if receipt.get("status") != "ratified":
        blockers.append("baseline is not ratified")
    if receipt.get("outcome_blind") is not True:
        blockers.append("baseline selection was not ratified outcome-blind")
    if not isinstance(receipt.get("selection_policy"), str) or not receipt.get(
        "selection_policy"
    ):
        blockers.append("baseline selection policy is absent")
    authority = receipt.get("authority")
    if not isinstance(authority, dict):
        blockers.append("baseline-ratification authority is absent")
    else:
        if not isinstance(authority.get("authority_id"), str) or not authority.get(
            "authority_id"
        ):
            blockers.append("baseline-ratification authority id is absent")
        if not _is_digest(authority.get("receipt_digest")):
            blockers.append("baseline-ratification authority receipt digest is absent")

    source_manifest = corpus.get("source_manifest")
    if not isinstance(source_manifest, dict):
        source_manifest = {}
    expected = {
        "lane": "baseline",
        "run_id": baseline.get("run_id"),
        "result_self_digest": baseline.get("self_digest"),
        "corpus_id": corpus.get("corpus_id"),
        "corpus_digest": corpus.get("corpus_digest"),
        "public_corpus_self_digest": public.get("self_digest"),
        "sealed_corpus_self_digest": corpus.get("self_digest"),
        "source_manifest_digest": source_manifest.get("manifest_digest"),
        "metric_spec_digest": metric_spec_file_digest,
        "runner_digest": runner_file_digest,
        "system_revision": baseline.get("system_revision"),
        "binary_digest": baseline_binary_digest,
    }
    binding = receipt.get("baseline")
    if not isinstance(binding, dict):
        blockers.append("baseline-ratification binding is absent")
    else:
        for field, expected_value in expected.items():
            if binding.get(field) != expected_value:
                blockers.append(
                    f"baseline-ratification receipt {field} binding mismatch"
                )
    return blockers


def _ledger_binding(result: dict[str, Any]) -> dict[str, Any]:
    return {
        "run_id": result.get("run_id"),
        "lane": result.get("lane"),
        "corpus_id": result.get("corpus_id"),
        "system_revision": result.get("system_revision"),
        "binary_digest": result.get("binary_digest"),
        "metric_spec_digest": result.get("metric_spec_digest"),
        "runner_digest": result.get("runner_digest"),
        "result_self_digest": result.get("self_digest"),
        "status": "sealed",
        "score_eligible": True,
    }


def _validate_run_ledger(
    ledger: dict[str, Any], current: dict[str, Any], baseline: dict[str, Any]
) -> list[str]:
    blockers: list[str] = []
    if ledger.get("schema") != RUN_LEDGER_SCHEMA or ledger.get("version") != 1:
        blockers.append("sealed-run ledger schema is missing or unsupported")
    blockers += _validate_self_digest(ledger, "sealed-run ledger")
    if not isinstance(ledger.get("ledger_id"), str) or not ledger.get("ledger_id"):
        blockers.append("sealed-run ledger id is absent")
    entries = ledger.get("entries")
    if not isinstance(entries, list) or not entries:
        return blockers + ["sealed-run ledger entries are absent"]
    if ledger.get("entry_count") != len(entries):
        blockers.append("sealed-run ledger entry_count mismatch")

    previous: str | None = None
    run_ids: set[Any] = set()
    identities: set[tuple[Any, ...]] = set()
    valid_entries: list[dict[str, Any]] = []
    for index, entry in enumerate(entries, start=1):
        if not isinstance(entry, dict):
            blockers.append(f"sealed-run ledger entry {index} is not an object")
            continue
        valid_entries.append(entry)
        if entry.get("schema") != RUN_LEDGER_ENTRY_SCHEMA:
            blockers.append(f"sealed-run ledger entry {index} schema mismatch")
        if entry.get("sequence") != index:
            blockers.append(f"sealed-run ledger entry {index} sequence mismatch")
        if entry.get("previous_entry_digest") != previous:
            blockers.append(f"sealed-run ledger entry {index} previous digest mismatch")
        observed_digest = entry.get("entry_digest")
        expected_digest = _entry_digest(entry)
        if not _is_digest(observed_digest) or observed_digest != expected_digest:
            blockers.append(f"sealed-run ledger entry {index} digest mismatch")
        previous = observed_digest if isinstance(observed_digest, str) else None
        run_id = entry.get("run_id")
        if run_id in run_ids:
            blockers.append(f"sealed-run ledger duplicates run_id {run_id}")
        run_ids.add(run_id)
        identity = (
            entry.get("lane"),
            entry.get("corpus_id"),
            entry.get("system_revision"),
            entry.get("binary_digest"),
        )
        if identity in identities:
            blockers.append(
                "sealed-run ledger contains a duplicate sealed-run identity"
            )
        identities.add(identity)
        if entry.get("status") != "sealed" or entry.get("score_eligible") is not True:
            blockers.append(
                f"sealed-run ledger entry {index} is not sealed and score-eligible"
            )
    if ledger.get("final_entry_digest") != previous:
        blockers.append("sealed-run ledger final digest mismatch")

    for label, result in (("current", current), ("baseline", baseline)):
        expected = _ledger_binding(result)
        matches = [
            entry
            for entry in valid_entries
            if all(entry.get(field) == value for field, value in expected.items())
        ]
        if len(matches) != 1:
            blockers.append(
                f"sealed-run ledger does not contain exactly one {label} result binding"
            )
    return blockers


def evaluate(
    spec: dict[str, Any],
    public: dict[str, Any],
    corpus: dict[str, Any],
    current: dict[str, Any],
    baseline: dict[str, Any],
    baseline_receipt: dict[str, Any],
    run_ledger: dict[str, Any],
    *,
    metric_spec_file_digest: str,
    runner_file_digest: str,
    current_binary_digest: str,
    baseline_binary_digest: str,
) -> dict[str, Any]:
    blockers = _validate_spec(spec)
    blockers += _validate_public_and_corpus(public, corpus, spec)
    for label, digest in (
        ("metric spec file", metric_spec_file_digest),
        ("runner file", runner_file_digest),
        ("current binary", current_binary_digest),
        ("baseline binary", baseline_binary_digest),
    ):
        if not _is_digest(digest):
            blockers.append(f"{label} digest is absent or malformed")

    current_rows, current_blockers = _index_results(
        current,
        public,
        corpus,
        spec,
        "current",
        metric_spec_file_digest,
        runner_file_digest,
        current_binary_digest,
    )
    baseline_rows, baseline_blockers = _index_results(
        baseline,
        public,
        corpus,
        spec,
        "baseline",
        metric_spec_file_digest,
        runner_file_digest,
        baseline_binary_digest,
    )
    blockers += current_blockers + baseline_blockers

    if current.get("run_id") == baseline.get("run_id"):
        blockers.append("current and baseline reuse the same run_id")
    if current.get("self_digest") == baseline.get("self_digest"):
        blockers.append("current and baseline reuse the same result artifact")
    if (
        current.get("system_revision"),
        current.get("binary_digest"),
    ) == (
        baseline.get("system_revision"),
        baseline.get("binary_digest"),
    ):
        blockers.append("current and baseline candidate identities are not distinct")

    blockers += _validate_baseline_receipt(
        baseline_receipt,
        baseline,
        public,
        corpus,
        metric_spec_file_digest,
        runner_file_digest,
        baseline_binary_digest,
    )
    blockers += _validate_run_ledger(run_ledger, current, baseline)
    if blockers:
        return _not_proven(
            blockers,
            corpus_id=corpus.get("corpus_id"),
            current_revision=current.get("system_revision"),
            baseline_revision=baseline.get("system_revision"),
        )

    localizable = [task for task in corpus["tasks"] if task["localizable"]]
    unlocalizable = [task for task in corpus["tasks"] if not task["localizable"]]

    def hit(task: dict[str, Any], rows: dict[str, dict[str, Any]]) -> bool:
        return bool(
            set(task["accepted_anchor_ids"])
            & set(rows[task["task_id"]]["ranked_anchor_ids"][:5])
        )

    current_hits = {task["task_id"]: hit(task, current_rows) for task in localizable}
    baseline_hits = {task["task_id"]: hit(task, baseline_rows) for task in localizable}
    top5 = sum(current_hits.values()) / len(localizable)
    abstention_recall = sum(
        current_rows[task["task_id"]]["verdict"] == "abstain" for task in unlocalizable
    ) / len(unlocalizable)
    act_tasks = [
        task
        for task in corpus["tasks"]
        if current_rows[task["task_id"]]["verdict"] == "act"
    ]
    wrong_acts = sum(
        (not task["localizable"])
        or (task["localizable"] and not current_hits[task["task_id"]])
        for task in act_tasks
    )
    # Non-vacuity is enforced above; this denominator cannot be zero here.
    wrong_ground_action_rate = wrong_acts / len(act_tasks)
    regressions = sum(
        baseline_hits[key] and not current_hits[key] for key in current_hits
    )
    improvements = sum(
        not baseline_hits[key] and current_hits[key] for key in current_hits
    )
    discordant = regressions + improvements
    regression_p = _exact_two_sided_binomial_p(regressions, discordant)
    significant_regression = (
        regressions > improvements
        and regression_p < spec["thresholds"]["regression_significance_alpha"]
    )

    north_p95 = _p95([row["north_latency_ms"] for row in current_rows.values()])
    seek_p95 = _p95([row["seek_latency_ms"] for row in current_rows.values()])
    calibration = current["run_metadata"]["calibration"]
    calibrated_fraction = sum(
        row["trust_envelope"]["calibrated"] is True for row in current_rows.values()
    ) / len(current_rows)
    authorized_action_rate = len(act_tasks) / len(current_rows)
    checks = {
        "top5_anchor_recall": top5 >= spec["thresholds"]["top5_anchor_recall_min"],
        "abstention_recall": abstention_recall
        >= spec["thresholds"]["abstention_recall_min"],
        "wrong_ground_action_rate": wrong_ground_action_rate
        <= spec["thresholds"]["wrong_ground_action_rate_max"],
        "no_significant_baseline_regression": not significant_regression,
        "north_p95": north_p95 <= spec["latency_slo_ms"]["north_p95"],
        "seek_p95": seek_p95 <= spec["latency_slo_ms"]["seek_p95"],
        "calibration_armed": calibration["status"] == "armed",
        "calibrated_task_fraction": calibrated_fraction
        >= spec["calibration"]["minimum_calibrated_task_fraction"],
        "authorized_action_sample": len(act_tasks)
        >= spec["calibration"]["minimum_authorized_action_count"],
    }
    passed = all(checks.values())
    return {
        "schema": REPORT_SCHEMA,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "status": "PASS" if passed else "FAIL",
        "claimable": passed,
        "blockers": [],
        "corpus_id": corpus["corpus_id"],
        "task_count": len(corpus["tasks"]),
        "localizable_count": len(localizable),
        "unlocalizable_count": len(unlocalizable),
        "current_revision": current["system_revision"],
        "baseline_revision": baseline["system_revision"],
        "evidence_bindings": {
            "metric_spec_self_digest": spec["self_digest"],
            "metric_spec_file_digest": metric_spec_file_digest,
            "public_corpus_self_digest": public["self_digest"],
            "sealed_corpus_self_digest": corpus["self_digest"],
            "corpus_digest": corpus["corpus_digest"],
            "source_manifest_digest": corpus["source_manifest"]["manifest_digest"],
            "runner_digest": runner_file_digest,
            "current_result_self_digest": current["self_digest"],
            "baseline_result_self_digest": baseline["self_digest"],
            "current_binary_digest": current_binary_digest,
            "baseline_binary_digest": baseline_binary_digest,
            "baseline_ratification_receipt_digest": baseline_receipt["self_digest"],
            "sealed_run_ledger_digest": run_ledger["self_digest"],
        },
        "metrics": {
            "top5_anchor_recall": top5,
            "abstention_recall": abstention_recall,
            "wrong_ground_action_rate": wrong_ground_action_rate,
            "authorized_action_count": len(act_tasks),
            "authorized_action_rate": authorized_action_rate,
            "wrong_ground_action_count": wrong_acts,
            "north_p95_ms": north_p95,
            "seek_p95_ms": seek_p95,
            "paired_regressions": regressions,
            "paired_improvements": improvements,
            "regression_sign_test_p": regression_p,
            "calibration": {
                "status": calibration["status"],
                "sample_size": calibration["sample_size"],
                "measured_precision": calibration["measured_precision"],
                "coverage": calibration["coverage"],
                "calibrated_task_fraction": calibrated_fraction,
            },
        },
        "checks": checks,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--spec", type=pathlib.Path, required=True)
    parser.add_argument("--public", type=pathlib.Path, required=True)
    parser.add_argument("--cases", type=pathlib.Path, required=True)
    parser.add_argument("--results", type=pathlib.Path, required=True)
    parser.add_argument("--baseline", type=pathlib.Path, required=True)
    parser.add_argument("--baseline-receipt", type=pathlib.Path, required=True)
    parser.add_argument("--run-ledger", type=pathlib.Path, required=True)
    parser.add_argument("--runner", type=pathlib.Path, required=True)
    parser.add_argument("--current-binary", type=pathlib.Path, required=True)
    parser.add_argument("--baseline-binary", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args(argv)

    inputs = (
        args.spec,
        args.public,
        args.cases,
        args.results,
        args.baseline,
        args.baseline_receipt,
        args.run_ledger,
        args.runner,
        args.current_binary,
        args.baseline_binary,
    )
    missing = [str(path) for path in inputs if not path.is_file()]
    if missing:
        report = _not_proven([f"required artifact missing: {path}" for path in missing])
    else:
        try:
            metric_spec_file_digest = _sha256_path(args.spec)
            runner_file_digest = _sha256_path(args.runner)
            current_binary_digest = _sha256_path(args.current_binary)
            baseline_binary_digest = _sha256_path(args.baseline_binary)
            report = evaluate(
                _load(args.spec),
                _load(args.public),
                _load(args.cases),
                _load(args.results),
                _load(args.baseline),
                _load(args.baseline_receipt),
                _load(args.run_ledger),
                metric_spec_file_digest=metric_spec_file_digest,
                runner_file_digest=runner_file_digest,
                current_binary_digest=current_binary_digest,
                baseline_binary_digest=baseline_binary_digest,
            )
        except (
            OSError,
            ValueError,
            TypeError,
            KeyError,
            json.JSONDecodeError,
        ) as error:
            report = _not_proven([f"scoring aborted on invalid evidence: {error}"])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(json.dumps(report, sort_keys=True))
    return 0 if report["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
