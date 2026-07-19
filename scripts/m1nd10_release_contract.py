#!/usr/bin/env python3
"""Canonical M1ND-10 release contracts, mirroring ``m1nd-control::release``.

This module is deliberately structural.  A successful validation means that
the JSON shape, canonical digest, candidate binding, and convergence laws are
valid; it never means that an opaque signature was cryptographically verified.
Real promotion authority must verify signatures and key lifecycle separately.
"""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path
from typing import Any, Iterable


CANONICALIZATION_VERSION = "m1nd-canonical-json-v1"
DIGEST_PREFIX = b"m1nd-domain-separated-sha256-v1\0"
RELEASE_CANDIDATE_SCHEMA = "m1nd-release-candidate-manifest-v1"
RELEASE_CANDIDATE_DIGEST_DOMAIN = RELEASE_CANDIDATE_SCHEMA
GATE_RECEIPT_SCHEMA = "m1nd-gate-receipt-v1"
GATE_RECEIPT_DIGEST_DOMAIN = GATE_RECEIPT_SCHEMA
INDEPENDENT_REVIEW_RECEIPT_SCHEMA = (
    "m1nd-independent-adversarial-review-receipt-v1"
)
INDEPENDENT_REVIEW_RECEIPT_DIGEST_DOMAIN = INDEPENDENT_REVIEW_RECEIPT_SCHEMA

# Rust ReleaseEvidenceSetV1 is memory-only (it has no serde wire contract).
# This is an explicit cross-language JSON extension, not a Rust-derived schema.
EVIDENCE_SET_JSON_EXTENSION_SCHEMA = "m1nd-release-evidence-set-json-extension-v1"
STRUCTURAL_STATUS = "STRUCTURALLY_VALID_NOT_CRYPTOGRAPHICALLY_VERIFIED"
FIXTURE_SIGNATURE_PREFIX = "NOT_CRYPTOGRAPHIC:"

GATE_IDS = tuple(f"G{index}" for index in range(11))
GATE_VERDICTS = {"PASS", "FAIL", "NOT_RUN", "NOT_PROVEN"}
FINDING_SEVERITIES = {"P0", "P1", "P2", "P3", "Info"}
FINDING_STATUSES = {"OPEN", "CLOSED"}
ACTIVE_MODES = {"HUMAN_GATED", "POLICY_AUTONOMOUS", "FULL_AUTONOMY"}

COMPATIBILITY_ARTIFACT_KEY = "release_compatibility_manifest_v1"
ROLLBACK_ARTIFACT_KEY = "release_rollback_plan_v1"
RELEASE_ASSET_ARTIFACT_PREFIX = "release_asset:"
RELEASE_ARTIFACT_PREFIX = "release_artifact:"

_HEX_64 = re.compile(r"[0-9A-Fa-f]{64}\Z")


class ReleaseContractError(ValueError):
    pass


def _reject_float(value: str) -> None:
    raise ReleaseContractError(f"non-integer JSON number refused: {value}")


def _reject_constant(value: str) -> None:
    raise ReleaseContractError(f"non-finite JSON number refused: {value}")


def _object_without_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ReleaseContractError(f"duplicate JSON object key refused: {key!r}")
        result[key] = value
    return result


def loads_integer_json(text: str) -> Any:
    """Parse unambiguous JSON without losing integers or accepting floats."""

    try:
        value = json.loads(
            text,
            parse_float=_reject_float,
            parse_constant=_reject_constant,
            object_pairs_hook=_object_without_duplicate_keys,
        )
        _validate_canonical_value(value)
        return value
    except json.JSONDecodeError as error:
        raise ReleaseContractError(f"invalid JSON: {error}") from error


def load_integer_json(path: Path, description: str) -> Any:
    try:
        return loads_integer_json(path.read_text(encoding="utf-8"))
    except OSError as error:
        raise ReleaseContractError(f"cannot read {description} {path}: {error}") from error


def _validate_canonical_value(value: Any, path: str = "$") -> None:
    if value is None or isinstance(value, bool):
        return
    if isinstance(value, str):
        try:
            value.encode("utf-8")
        except UnicodeEncodeError as error:
            raise ReleaseContractError(
                f"canonical JSON string contains an unpaired surrogate at {path}"
            ) from error
        return
    if isinstance(value, int):
        return
    if isinstance(value, list):
        for index, item in enumerate(value):
            _validate_canonical_value(item, f"{path}[{index}]")
        return
    if isinstance(value, dict):
        for key, item in value.items():
            if not isinstance(key, str):
                raise ReleaseContractError(f"canonical JSON object key is not text at {path}")
            _validate_canonical_value(key, f"{path}.<key>")
            _validate_canonical_value(item, f"{path}.{key}")
        return
    raise ReleaseContractError(
        f"canonical JSON supports only objects, arrays, strings, integers, booleans, and null; "
        f"got {type(value).__name__} at {path}"
    )


def canonical_json(value: Any) -> bytes:
    """Return Rust-compatible canonical JSON: UTF-8, sorted keys, no newline."""

    _validate_canonical_value(value)
    return json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
        allow_nan=False,
    ).encode("utf-8")


def digest_domain_bytes(domain: str, payload: bytes) -> str:
    domain_bytes = domain.encode("utf-8")
    framed = b"".join(
        (
            DIGEST_PREFIX,
            len(domain_bytes).to_bytes(8, "big"),
            domain_bytes,
            len(payload).to_bytes(8, "big"),
            payload,
        )
    )
    return hashlib.sha256(framed).hexdigest()


def digest_canonical(domain: str, value: Any) -> str:
    return digest_domain_bytes(domain, canonical_json(value))


def _object(value: Any, description: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ReleaseContractError(f"{description} must be a JSON object")
    if not all(isinstance(key, str) for key in value):
        raise ReleaseContractError(f"{description} contains a non-text key")
    return value


def _exact_keys(value: dict[str, Any], expected: Iterable[str], description: str) -> None:
    expected_set = set(expected)
    actual_set = set(value)
    if actual_set != expected_set:
        missing = sorted(expected_set - actual_set)
        unknown = sorted(actual_set - expected_set)
        raise ReleaseContractError(
            f"{description} fields differ: missing={missing}, unknown={unknown}"
        )


def _text(value: Any, field: str, *, trim: bool = True) -> str:
    if not isinstance(value, str) or (not value.strip() if trim else value == ""):
        raise ReleaseContractError(f"required field '{field}' is empty or not text")
    return value


def _digest(value: Any, field: str) -> str:
    if not isinstance(value, str) or _HEX_64.fullmatch(value) is None:
        raise ReleaseContractError(
            f"required digest '{field}' is not 64 hexadecimal characters"
        )
    return value


def _u64(value: Any, field: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or not 0 <= value <= 2**64 - 1:
        raise ReleaseContractError(f"{field} must be a u64 integer")
    return value


def _i32_or_none(value: Any, field: str) -> int | None:
    if value is None:
        return None
    if isinstance(value, bool) or not isinstance(value, int) or not -(2**31) <= value < 2**31:
        raise ReleaseContractError(f"{field} must be null or an i32 integer")
    return value


def _named_map(
    value: Any,
    field: str,
    *,
    digest_values: bool,
) -> dict[str, str]:
    result = _object(value, field)
    if not result:
        raise ReleaseContractError(f"required map '{field}' is empty")
    for name, item in result.items():
        _text(name, f"{field}.key")
        if digest_values:
            _digest(item, f"{field}.{name}")
        else:
            _text(item, f"{field}.{name}")
    return result  # type: ignore[return-value]


def _signature(value: Any, field: str) -> str:
    # Exact Rust law: OpaqueSignature::is_empty(), not trim().  Whitespace is
    # structurally present even though it carries no authentication semantics.
    return _text(value, field, trim=False)


def _finding(value: Any) -> dict[str, Any]:
    finding = _object(value, "release finding")
    _exact_keys(
        finding,
        ("finding_id", "severity", "status", "statement", "evidence_digest"),
        "release finding",
    )
    _text(finding["finding_id"], "finding_id")
    if finding["severity"] not in FINDING_SEVERITIES:
        raise ReleaseContractError(f"invalid finding severity: {finding['severity']!r}")
    if finding["status"] not in FINDING_STATUSES:
        raise ReleaseContractError(f"invalid finding status: {finding['status']!r}")
    _text(finding["statement"], "finding.statement")
    _digest(finding["evidence_digest"], "finding.evidence_digest")
    return finding


def _findings(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        raise ReleaseContractError("findings must be an array")
    seen: set[str] = set()
    result = []
    for raw in value:
        finding = _finding(raw)
        finding_id = finding["finding_id"]
        if finding_id in seen:
            raise ReleaseContractError(f"duplicate finding id '{finding_id}'")
        seen.add(finding_id)
        result.append(finding)
    return result


CANDIDATE_CORE_FIELDS = (
    "repo_commits",
    "artifact_digests",
    "schema_policy_versions",
    "tool_catalog_digest",
    "safety_kernel_digest",
    "previous_governance_runtime_digest",
    "constitution_epoch_digest",
    "autonomy_epoch_grants_digest",
    "independence_quorum_policy_digest",
    "intended_active_mode",
    "compatibility_manifest_digest",
    "rollback_plan_digest",
    "harness_fixture_threat_digests",
    "build_environment_digest",
    "built_at",
)


def validate_candidate_core(core_value: Any) -> dict[str, Any]:
    core = _object(core_value, "release candidate core")
    _exact_keys(core, CANDIDATE_CORE_FIELDS, "release candidate core")
    _named_map(core["repo_commits"], "repo_commits", digest_values=False)
    _named_map(core["artifact_digests"], "artifact_digests", digest_values=True)
    _named_map(core["schema_policy_versions"], "schema_policy_versions", digest_values=False)
    _named_map(
        core["harness_fixture_threat_digests"],
        "harness_fixture_threat_digests",
        digest_values=True,
    )
    for field in (
        "tool_catalog_digest",
        "safety_kernel_digest",
        "previous_governance_runtime_digest",
        "constitution_epoch_digest",
        "autonomy_epoch_grants_digest",
        "independence_quorum_policy_digest",
        "compatibility_manifest_digest",
        "rollback_plan_digest",
        "build_environment_digest",
    ):
        _digest(core[field], field)
    if core["intended_active_mode"] not in ACTIVE_MODES:
        raise ReleaseContractError(
            f"invalid intended_active_mode: {core['intended_active_mode']!r}"
        )
    _u64(core["built_at"], "built_at")
    return core


def validate_operational_artifact_keys(core_value: Any) -> dict[str, Any]:
    """Validate the updater naming extension layered above the exact Rust core."""

    core = validate_candidate_core(core_value)
    artifact_digests = core["artifact_digests"]
    if artifact_digests.get(COMPATIBILITY_ARTIFACT_KEY) != core[
        "compatibility_manifest_digest"
    ]:
        raise ReleaseContractError(
            f"artifact_digests.{COMPATIBILITY_ARTIFACT_KEY} must equal compatibility_manifest_digest"
        )
    if artifact_digests.get(ROLLBACK_ARTIFACT_KEY) != core["rollback_plan_digest"]:
        raise ReleaseContractError(
            f"artifact_digests.{ROLLBACK_ARTIFACT_KEY} must equal rollback_plan_digest"
        )
    return core


def seal_candidate(core: Any, provenance_signature: str) -> dict[str, Any]:
    validated = validate_candidate_core(core)
    _signature(provenance_signature, "provenance_signature")
    candidate = {
        "schema": RELEASE_CANDIDATE_SCHEMA,
        "core": validated,
        "candidate_digest": digest_canonical(RELEASE_CANDIDATE_DIGEST_DOMAIN, validated),
        "provenance_signature": provenance_signature,
    }
    validate_candidate(candidate)
    return candidate


def validate_candidate(value: Any) -> dict[str, Any]:
    candidate = _object(value, "release candidate")
    _exact_keys(
        candidate,
        ("schema", "core", "candidate_digest", "provenance_signature"),
        "release candidate",
    )
    if candidate["schema"] != RELEASE_CANDIDATE_SCHEMA:
        raise ReleaseContractError("invalid release candidate schema")
    core = validate_candidate_core(candidate["core"])
    observed = _digest(candidate["candidate_digest"], "candidate_digest")
    _signature(candidate["provenance_signature"], "provenance_signature")
    expected = digest_canonical(RELEASE_CANDIDATE_DIGEST_DOMAIN, core)
    if observed != expected:
        raise ReleaseContractError(
            f"candidate_digest mismatch: expected={expected}, actual={observed}"
        )
    return candidate


GATE_CORE_FIELDS = (
    "candidate_digest",
    "gate_id",
    "spec_version",
    "metric_spec_digest",
    "harness_fixture_digest",
    "environment_digest",
    "provider_id",
    "provider_key_version",
    "input_digests",
    "command",
    "started_at",
    "ended_at",
    "exit_code",
    "verdict",
    "findings",
    "artifact_digests",
)


def validate_gate_core(core_value: Any) -> dict[str, Any]:
    core = _object(core_value, "gate receipt core")
    _exact_keys(core, GATE_CORE_FIELDS, "gate receipt core")
    _digest(core["candidate_digest"], "candidate_digest")
    if core["gate_id"] not in GATE_IDS:
        raise ReleaseContractError(f"invalid gate_id: {core['gate_id']!r}")
    _text(core["spec_version"], "spec_version")
    if core["metric_spec_digest"] is not None:
        _digest(core["metric_spec_digest"], "metric_spec_digest")
    _digest(core["harness_fixture_digest"], "harness_fixture_digest")
    _digest(core["environment_digest"], "environment_digest")
    _text(core["provider_id"], "provider_id")
    _text(core["provider_key_version"], "provider_key_version")
    _named_map(core["input_digests"], "input_digests", digest_values=True)
    _text(core["command"], "command")
    started = _u64(core["started_at"], "started_at")
    ended = _u64(core["ended_at"], "ended_at")
    if ended < started:
        raise ReleaseContractError("invalid gate time window")
    exit_code = _i32_or_none(core["exit_code"], "exit_code")
    if core["verdict"] not in GATE_VERDICTS:
        raise ReleaseContractError(f"invalid gate verdict: {core['verdict']!r}")
    if core["verdict"] == "PASS" and exit_code != 0:
        raise ReleaseContractError("PASS requires exit_code=0")
    if core["verdict"] == "NOT_RUN" and exit_code is not None:
        raise ReleaseContractError("NOT_RUN cannot claim an exit code")
    _findings(core["findings"])
    _named_map(core["artifact_digests"], "artifact_digests", digest_values=True)
    return core


def seal_gate_receipt(core: Any, signature: str) -> dict[str, Any]:
    validated = validate_gate_core(core)
    _signature(signature, "signature")
    digest = digest_canonical(GATE_RECEIPT_DIGEST_DOMAIN, validated)
    receipt = {
        "schema": GATE_RECEIPT_SCHEMA,
        "core": validated,
        "receipt_id": f"gate:{digest}",
        "receipt_digest": digest,
        "signature": signature,
    }
    validate_gate_receipt(receipt)
    return receipt


def validate_gate_receipt(value: Any) -> dict[str, Any]:
    receipt = _object(value, "gate receipt")
    _exact_keys(
        receipt,
        ("schema", "core", "receipt_id", "receipt_digest", "signature"),
        "gate receipt",
    )
    if receipt["schema"] != GATE_RECEIPT_SCHEMA:
        raise ReleaseContractError("invalid gate receipt schema")
    core = validate_gate_core(receipt["core"])
    digest = _digest(receipt["receipt_digest"], "receipt_digest")
    if receipt["receipt_id"] != f"gate:{digest}":
        raise ReleaseContractError("gate receipt_id mismatch")
    _signature(receipt["signature"], "signature")
    expected = digest_canonical(GATE_RECEIPT_DIGEST_DOMAIN, core)
    if digest != expected:
        raise ReleaseContractError(
            f"receipt_digest mismatch: expected={expected}, actual={digest}"
        )
    return receipt


REVIEW_CORE_FIELDS = (
    "candidate_digest",
    "threat_matrix_digest",
    "provider_id",
    "provider_model_version",
    "provider_key_version",
    "reviewed_inputs_digest",
    "binding_changes",
    "started_at",
    "ended_at",
    "verdict",
    "findings",
)


def validate_review_core(core_value: Any) -> dict[str, Any]:
    core = _object(core_value, "independent review core")
    _exact_keys(core, REVIEW_CORE_FIELDS, "independent review core")
    _digest(core["candidate_digest"], "candidate_digest")
    _digest(core["threat_matrix_digest"], "threat_matrix_digest")
    _text(core["provider_id"], "provider_id")
    _text(core["provider_model_version"], "provider_model_version")
    _text(core["provider_key_version"], "provider_key_version")
    _digest(core["reviewed_inputs_digest"], "reviewed_inputs_digest")
    if not isinstance(core["binding_changes"], list) or not all(
        isinstance(item, str) for item in core["binding_changes"]
    ):
        raise ReleaseContractError("binding_changes must be an array of strings")
    started = _u64(core["started_at"], "started_at")
    ended = _u64(core["ended_at"], "ended_at")
    if ended < started:
        raise ReleaseContractError("invalid independent review time window")
    if core["verdict"] not in GATE_VERDICTS:
        raise ReleaseContractError(f"invalid review verdict: {core['verdict']!r}")
    _findings(core["findings"])
    return core


def seal_independent_review(core: Any, signature: str) -> dict[str, Any]:
    validated = validate_review_core(core)
    _signature(signature, "signature")
    digest = digest_canonical(INDEPENDENT_REVIEW_RECEIPT_DIGEST_DOMAIN, validated)
    receipt = {
        "schema": INDEPENDENT_REVIEW_RECEIPT_SCHEMA,
        "core": validated,
        "receipt_id": f"iar:{digest}",
        "receipt_digest": digest,
        "signature": signature,
    }
    validate_independent_review(receipt)
    return receipt


def validate_independent_review(value: Any) -> dict[str, Any]:
    receipt = _object(value, "independent review receipt")
    _exact_keys(
        receipt,
        ("schema", "core", "receipt_id", "receipt_digest", "signature"),
        "independent review receipt",
    )
    if receipt["schema"] != INDEPENDENT_REVIEW_RECEIPT_SCHEMA:
        raise ReleaseContractError("invalid independent review receipt schema")
    core = validate_review_core(receipt["core"])
    digest = _digest(receipt["receipt_digest"], "receipt_digest")
    if receipt["receipt_id"] != f"iar:{digest}":
        raise ReleaseContractError("independent review receipt_id mismatch")
    _signature(receipt["signature"], "signature")
    expected = digest_canonical(INDEPENDENT_REVIEW_RECEIPT_DIGEST_DOMAIN, core)
    if digest != expected:
        raise ReleaseContractError(
            f"independent review digest mismatch: expected={expected}, actual={digest}"
        )
    return receipt


def _has_open_p0_or_p1(findings: list[dict[str, Any]]) -> bool:
    return any(
        finding["status"] == "OPEN" and finding["severity"] in {"P0", "P1"}
        for finding in findings
    )


def evidence_set_json_extension(
    candidate: Any,
    gate_receipts: list[Any],
    independent_review: Any,
) -> dict[str, Any]:
    evidence = {
        "schema": EVIDENCE_SET_JSON_EXTENSION_SCHEMA,
        "contract_status": STRUCTURAL_STATUS,
        "candidate": candidate,
        "gate_receipts": gate_receipts,
        "independent_review": independent_review,
    }
    validate_convergence(evidence)
    return evidence


def validate_convergence(value: Any) -> dict[str, Any]:
    evidence = _object(value, "release evidence-set JSON extension")
    _exact_keys(
        evidence,
        ("schema", "contract_status", "candidate", "gate_receipts", "independent_review"),
        "release evidence-set JSON extension",
    )
    if evidence["schema"] != EVIDENCE_SET_JSON_EXTENSION_SCHEMA:
        raise ReleaseContractError("invalid evidence-set JSON extension schema")
    if evidence["contract_status"] != STRUCTURAL_STATUS:
        raise ReleaseContractError("evidence-set must disclose structural-only status")
    candidate = validate_candidate(evidence["candidate"])
    review = validate_independent_review(evidence["independent_review"])
    if review["core"]["candidate_digest"] != candidate["candidate_digest"]:
        raise ReleaseContractError("independent review candidate mismatch")
    if review["core"]["verdict"] != "PASS":
        raise ReleaseContractError("independent adversarial review is not PASS")
    if _has_open_p0_or_p1(review["core"]["findings"]):
        raise ReleaseContractError("independent review has an open P0/P1")
    if not isinstance(evidence["gate_receipts"], list):
        raise ReleaseContractError("gate_receipts must be an array")
    observed: set[str] = set()
    for raw in evidence["gate_receipts"]:
        receipt = validate_gate_receipt(raw)
        gate = receipt["core"]["gate_id"]
        if receipt["core"]["candidate_digest"] != candidate["candidate_digest"]:
            raise ReleaseContractError(f"{gate} candidate mismatch")
        if gate in observed:
            raise ReleaseContractError(f"duplicate gate {gate}")
        observed.add(gate)
        if receipt["core"]["verdict"] != "PASS":
            raise ReleaseContractError(f"{gate} is not PASS")
        if _has_open_p0_or_p1(receipt["core"]["findings"]):
            raise ReleaseContractError(f"{gate} has an open P0/P1")
    missing = [gate for gate in GATE_IDS if gate not in observed]
    if missing:
        raise ReleaseContractError(f"missing gates: {missing}")
    return evidence


def verify_vectors(value: Any) -> dict[str, Any]:
    vectors = _object(value, "cross-language release vectors")
    if vectors.get("schema") != "m1nd-release-cross-language-vectors-v1":
        raise ReleaseContractError("invalid cross-language vector schema")
    if vectors.get("canonicalization_version") != CANONICALIZATION_VERSION:
        raise ReleaseContractError("canonicalization vector version mismatch")
    enum_values = _object(vectors.get("enum_wire_values"), "enum wire values")
    if enum_values != {
        "active_mode": sorted(ACTIVE_MODES),
        "finding_severity": ["P0", "P1", "P2", "P3", "Info"],
        "finding_status": sorted(FINDING_STATUSES),
        "gate_id": list(GATE_IDS),
        "gate_verdict": ["PASS", "FAIL", "NOT_RUN", "NOT_PROVEN"],
    }:
        raise ReleaseContractError("enum wire vectors drifted")
    keys = _object(vectors.get("artifact_digest_keys"), "artifact digest keys")
    if keys != {
        "compatibility": COMPATIBILITY_ARTIFACT_KEY,
        "release_artifact_prefix": RELEASE_ARTIFACT_PREFIX,
        "release_asset_prefix": RELEASE_ASSET_ARTIFACT_PREFIX,
        "rollback": ROLLBACK_ARTIFACT_KEY,
    }:
        raise ReleaseContractError("artifact digest key vectors drifted")
    cases = vectors.get("canonical_cases")
    if not isinstance(cases, list) or not cases:
        raise ReleaseContractError("canonical_cases must be a non-empty array")
    for item in cases:
        case = _object(item, "canonical case")
        text = canonical_json(case["value"]).decode("utf-8")
        if text != case["canonical_json"]:
            raise ReleaseContractError(f"canonical text mismatch for {case.get('name')}")
        digest = digest_canonical(case["domain"], case["value"])
        if digest != case["digest"]:
            raise ReleaseContractError(f"canonical digest mismatch for {case.get('name')}")
    for item in vectors.get("refusal_cases", []):
        case = _object(item, "refusal case")
        try:
            loads_integer_json(case["json"])
        except ReleaseContractError:
            pass
        else:
            raise ReleaseContractError(f"refusal case was accepted: {case.get('name')}")
    operational = _object(vectors.get("operational_manifests"), "operational manifests")
    compatibility_bytes = canonical_json(operational["compatibility"])
    rollback_bytes = canonical_json(operational["rollback"])
    if hashlib.sha256(compatibility_bytes).hexdigest() != operational[
        "compatibility_sha256"
    ]:
        raise ReleaseContractError("compatibility vector digest mismatch")
    if hashlib.sha256(rollback_bytes).hexdigest() != operational["rollback_sha256"]:
        raise ReleaseContractError("rollback vector digest mismatch")
    candidate = vectors["evidence_set"]["candidate"]
    validate_operational_artifact_keys(candidate["core"])
    if candidate["core"]["compatibility_manifest_digest"] != operational[
        "compatibility_sha256"
    ]:
        raise ReleaseContractError("candidate does not bind compatibility vector")
    if candidate["core"]["rollback_plan_digest"] != operational["rollback_sha256"]:
        raise ReleaseContractError("candidate does not bind rollback vector")
    validate_convergence(vectors["evidence_set"])
    return vectors
