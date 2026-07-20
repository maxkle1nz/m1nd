#!/usr/bin/env python3

from __future__ import annotations

import json
import os
import pathlib
import sys
import tempfile
import time
import unittest
from unittest import mock


sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from m1nd10_g6_blind_runner import (  # noqa: E402
    AUTHORITY_AUTHORIZE_REQUEST_SCHEMA,
    AUTHORITY_AUTHORIZE_RESPONSE_SCHEMA,
    AUTHORITY_ASSEMBLY_SCHEMA,
    AUTHORITY_PREFLIGHT_RESPONSE_SCHEMA,
    AUTHORITY_PROVIDER_RESPONSE_SCHEMA,
    AUTHORIZATION_VERIFIER_RESPONSE_SCHEMA,
    AUTHORIZATION_RECEIPT_SCHEMA,
    AUTHORIZATION_RECEIPT_DIGEST_DOMAIN,
    CHECKPOINT_ACK_SCHEMA,
    CODE_OWNERSHIP_MANIFEST_SCHEMA,
    CODE_PIPELINE_RECEIPT_SCHEMA,
    EXTERNAL_MUTATION_RESPONSE_SCHEMA,
    GRAPH_PREVIEW_RESPONSE_SCHEMA,
    GRAPH_INGEST_OUTCOME_DIGEST_DOMAIN,
    HISTORICAL_PUBLIC_SCHEMA,
    PUBLIC_SCHEMA,
    RESULT_SCHEMA,
    SEEK_CALIBRATION_DIGEST_DOMAIN,
    SEEK_CALIBRATION_RECEIPT_SCHEMA,
    AuthorityAssembly,
    AuthorityVerificationKey,
    ExternalAuthorityProvider,
    McpHttpClient,
    OwnerSpec,
    RunnerError,
    _canonical_bytes,
    _authority_assembly_digest,
    _process_group_popen_kwargs,
    _rust_domain_digest,
    _recomputed_ownership_digests,
    _sha256_bytes,
    _self_digest,
    _validate_owner_attestation,
    _validate_registry_entry,
    _without_key,
    _write_json_durable,
    authority_run_metadata,
    build_runner_checkpoint,
    build_owner_specs,
    capture_private_bearer,
    execute_governed_graph_ingest,
    extract_measurement,
    preflight_authority_provider,
    load_authority_assembly,
    public_source_revision,
    validate_calibration_receipt,
    validate_public_queries,
    validate_runner_checkpoint,
    validate_runner_paths,
    validate_unscored_artifact,
    verify_public_source_snapshot,
)


ROOT = pathlib.Path(__file__).resolve().parents[2]
REAL_PUBLIC = (
    ROOT / "docs" / "benchmarks" / "m1nd10-g6-held-out-v2" / "public" / "queries.json"
)
FIXTURE_NOW_MS = int(time.time_ns() // 1_000_000)
FIXTURE_SIGNATURE = "01" * 64
FIXTURE_PUBLIC_KEY = "02" * 32


def digest(value: int) -> str:
    return f"{value:064x}"


def sha_digest(value: int) -> str:
    return f"sha256:{digest(value)}"


def seal(value: dict) -> dict:
    value["self_digest"] = _self_digest(value)
    return value


def wire_calibration_receipt() -> dict:
    receipt = {
        "schema": SEEK_CALIBRATION_RECEIPT_SCHEMA,
        "status": "calibrated",
        "signal": "envelope",
        "receipt_digest": "",
        "tau": 0.73,
        "sample_size": 100,
        "measured_precision": 0.995,
        "coverage": 0.42,
        "target_alpha": 0.01,
        "calibrated_at_ms": 1_721_376_000_000,
    }
    receipt["receipt_digest"] = _rust_domain_digest(
        SEEK_CALIBRATION_DIGEST_DOMAIN,
        {
            "schema": receipt["schema"],
            "status": receipt["status"],
            "signal": receipt["signal"],
            "calibration_row": {
                "tau": receipt["tau"],
                "target_alpha": receipt["target_alpha"],
                "measured_precision": receipt["measured_precision"],
                "coverage": receipt["coverage"],
                "n": receipt["sample_size"],
                "calibrated_at_ms": receipt["calibrated_at_ms"],
            },
        },
    )
    return receipt


def owner_spec(root: pathlib.Path) -> OwnerSpec:
    return OwnerSpec(
        repo_id="repo-1",
        source_revision="revision-1",
        file_set_digest=sha_digest(40),
        root=root.resolve(),
        runtime_dir=(root / "runtime").resolve(),
        registry_dir=(root / "registry").resolve(),
        port=18100,
        owner_id="g6-owner-1-repo-1",
        scope=f"graph.ingest.replace:{root.resolve()}",
        source_digests=(
            ("src/a.rs", digest(30)),
            ("src/b.rs", digest(31)),
            ("Cargo.toml", digest(32)),
        ),
    )


def assembly(*, production: bool = True) -> AuthorityAssembly:
    key = AuthorityVerificationKey(
        key_id="owner-key-1" if production else "software-test-key",
        subject_id="owner-production-1" if production else "software-test-owner",
        algorithm="ED25519" if production else "SOFTWARE_TEST_NOT_PROVEN",
        public_key=FIXTURE_PUBLIC_KEY if production else "software-test-public-key",
        created_at=FIXTURE_NOW_MS - 100_000,
        activated_at=FIXTURE_NOW_MS - 90_000,
        expires_at=FIXTURE_NOW_MS + 600_000,
        revoked_at=None,
        rotated_at=None,
        replacement_key_id=None,
        status="ACTIVE",
    )
    return AuthorityAssembly(
        provider_kind="production" if production else "software_test",
        production_authority_assembly=production,
        assembly_id="authority-assembly-1",
        assembly_digest=digest(41),
        binary_digest=sha_digest(42),
        provider_executable_digest=sha_digest(43),
        owner_security_config_digest=sha_digest(44),
        key_registry_epoch=1,
        max_future_clock_skew_ms=30_000,
        verification_key=key,
        expected_digest_verified=True,
        blind_boundary_kind="test-blind-boundary-v1",
        blind_boundary_proven=True,
    )


def assembly_document(*, now_ms: int = FIXTURE_NOW_MS) -> dict:
    document = {
        "schema": AUTHORITY_ASSEMBLY_SCHEMA,
        "assembly_id": "authority-assembly-1",
        "provider_kind": "production",
        "production_authority_assembly": True,
        "owner_binary_digest": sha_digest(42),
        "provider_executable_digest": sha_digest(43),
        "owner_security_config_digest": sha_digest(44),
        "verification_key_registry": {
            "schema": "m1nd-verification-key-registry-v1",
            "registry_epoch": 1,
            "keys": {
                "owner-key-1": {
                    "key_id": "owner-key-1",
                    "subject_id": "owner-production-1",
                    "algorithm": "ED25519",
                    "public_key": FIXTURE_PUBLIC_KEY,
                    "created_at": now_ms - 100_000,
                    "activated_at": now_ms - 90_000,
                    "expires_at": now_ms + 600_000,
                    "revoked_at": None,
                    "rotated_at": None,
                    "replacement_key_id": None,
                    "status": "ACTIVE",
                }
            },
        },
        "receipt_key_id": "owner-key-1",
        "max_future_clock_skew_ms": 30_000,
        "self_digest": "",
    }
    document["self_digest"] = _authority_assembly_digest(document)
    return document


class RecordingVerifier:
    def __init__(self, *, accept_signature: str = FIXTURE_SIGNATURE) -> None:
        self.accept_signature = accept_signature
        self.requests: list[tuple[dict, AuthorityVerificationKey, int]] = []

    def verify(
        self, receipt: dict, key: AuthorityVerificationKey, max_skew: int
    ) -> dict:
        self.requests.append((receipt, key, max_skew))
        if receipt["signature"] != self.accept_signature:
            raise RunnerError("authorization receipt cryptographic verification failed")
        checked_at = receipt["core"]["authorized_at"] + 1
        return {
            "schema": AUTHORIZATION_VERIFIER_RESPONSE_SCHEMA,
            "status": "VERIFIED",
            "checked_at_ms": checked_at,
            "receipt_digest": receipt["receipt_digest"],
            "issuer": receipt["issuer"],
            "key_id": receipt["key_id"],
            "algorithm": receipt["algorithm"],
            "signature_verified": True,
            "clock_verified": True,
            "key_lifecycle_verified": True,
        }


def preview_for(spec: OwnerSpec, session_id: str) -> dict:
    effects = ["GRAPH_MUTATION", "RUNTIME_STORE_WRITE", "SOVEREIGN_MUTATION"]
    request_id = f"g6-preview-current-{spec.owner_id}"
    preview = {
        "schema": GRAPH_PREVIEW_RESPONSE_SCHEMA,
        "request_id": request_id,
        "preview_id": digest(1),
        "semantic_action": "graph.ingest.replace",
        "requested_effects": effects,
        "authority_floor": "POSITIVE_SOVEREIGN",
        "risk_class": "CRITICAL",
        "ingress": "MCP",
        "route_selector": str(spec.root),
        "actor_brain_id": "brain-owner-1",
        "transport_session_id": session_id,
        "ingress_context_digest": digest(2),
        "root_identity": str(spec.root),
        "expected_graph_generation": 0,
        "expected_source_projection_digest": digest(3),
        "candidate_ownership_digest": digest(4),
        "candidate_source_projection_digest": digest(5),
        "candidate_pipeline_digest": digest(6),
        "scan_job_id": "graph-ingest-scan-1",
        "semantic_payload_digest": digest(7),
        "operation_object_digest": digest(8),
        "authority_binding": {
            "target_action": "graph.ingest.replace",
            "payload_digest": digest(8),
            "requested_effects": effects,
            "mission_id": None,
            "mission_head_id": None,
        },
        "execute_request": {
            "action": "graph_ingest_replace",
            "schema": "m1nd-external-mutation-request-v1",
            "request_id": request_id,
            "request": {
                "preview_id": digest(1),
                "root": str(spec.root),
                "expected_graph_generation": 0,
                "expected_source_projection_digest": digest(3),
                "include_dotfiles": False,
                "dotfile_patterns": [],
                "parent": None,
            },
        },
    }
    return preview


def production_authorization_receipt(preview: dict, *, production: bool = True) -> dict:
    core = {
        "organism_id": "organism-1",
        "repo_id": "runtime-repo-1",
        "brain_id": preview["actor_brain_id"],
        "subject_id": "subject-1",
        "role": "AUTHOR",
        "capability_id": "capability-1",
        "capability_kind": "HUMAN",
        "verified_object_digest": preview["operation_object_digest"],
        "mission_id": None,
        "mission_head_id": None,
        "transport_session_id": preview["transport_session_id"],
        "ingress_context_digest": preview["ingress_context_digest"],
        "action": preview["semantic_action"],
        "ingress": "MCP",
        "complete_effects": preview["requested_effects"],
        "active_mode": "HUMAN_GATED",
        "constitution_digest": digest(50),
        "constitution_epoch": 1,
        "autonomy_epoch": 1,
        "protected_epoch_at_decision": 1,
        "policy_registry_digest": digest(51),
        "exact_policy_tuple": {
            "ingress": "MCP",
            "action": preview["semantic_action"],
            "active_mode": "HUMAN_GATED",
            "subject_id": "subject-1",
            "authority_variant": "HUMAN",
            "applicable_grant_id": None,
            "applicable_tier": None,
            "risk_class": "CRITICAL",
        },
        "authority_decision_digest": digest(52),
        "autonomy_admission_receipt_digest": None,
        "autonomy_committed_state_digest": None,
        "autonomy_protected_root_digest": None,
        "authority": {
            "POSITIVE": {
                "variant": "HUMAN",
                "assurance": (
                    "CONTROL_VERIFIED_ED25519"
                    if production
                    else "SOFTWARE_TEST_ONLY_NOT_PROVEN"
                ),
            }
        },
        "authority_body_digest": digest(53),
        "replay_sequence": 1,
        "journal_sequence": 1,
        "journal_root_digest": digest(54),
        "protected_epoch": 1,
        "authorized_at": FIXTURE_NOW_MS - 1_000,
        "expires_at": FIXTURE_NOW_MS + 60_000,
    }
    return {
        "schema": AUTHORIZATION_RECEIPT_SCHEMA,
        "core": core,
        "receipt_digest": _rust_domain_digest(
            AUTHORIZATION_RECEIPT_DIGEST_DOMAIN, core
        ),
        "issuer": "owner-production-1" if production else "software-test-owner",
        "key_id": "owner-key-1" if production else "software-test-key",
        "algorithm": "ED25519" if production else "SOFTWARE_TEST_NOT_PROVEN",
        "signature": FIXTURE_SIGNATURE if production else "software-test-signature",
    }


def pipeline_receipt(spec: OwnerSpec, binary_digest: str) -> dict:
    count = len(spec.source_digests)
    return {
        "schema": CODE_PIPELINE_RECEIPT_SCHEMA,
        "pipeline_version": "pipeline-v1",
        "producer_name": "m1nd-ingest",
        "producer_version": "1.0.0",
        "producer_build_identity": digest(60),
        "producer_executable_identity": binary_digest.removeprefix("sha256:"),
        "skip_dirs": [],
        "skip_files": [],
        "include_dotfiles": False,
        "dotfile_patterns": [],
        "policy_fingerprint": digest(61),
        "build_features": [],
        "binary_policy": "nul-in-first-8192-v1",
        "vcs_context_digest": digest(62),
        "immutable_source_snapshot": True,
        "discovered_source_count": count,
        "extracted_source_count": count,
        "digested_source_count": count,
        "global_enrichment_enabled": True,
        "cross_file_source_files_expected": 2,
        "cross_file_source_metadata_verified": 2,
        "cross_file_source_files_read": 2,
        "cross_file_source_files_parsed": 2,
        "cargo_workspace_members_expected": 1,
        "cargo_workspace_members_accounted": 1,
        "cargo_dependency_inputs_expected": 1,
        "cargo_dependency_inputs_accounted": 1,
        "cargo_package_file_links_expected": 1,
        "cargo_package_file_links_accounted": 1,
    }


def ownership_manifest(spec: OwnerSpec, preview: dict, binary_digest: str) -> dict:
    manifest = {
        "schema": CODE_OWNERSHIP_MANIFEST_SCHEMA,
        "root_identity": str(spec.root),
        "exact_source_key": None,
        "base_ownership_digest": None,
        "source_digests": dict(spec.source_digests),
        "claims_by_source": {
            path: {"source_hint": path, "node_ids": [], "edges": []}
            for path, _digest in spec.source_digests
        },
        "source_projection_digest": preview["candidate_source_projection_digest"],
        "graph_finalized": True,
        "pending_edge_count": 0,
        "bidirectional_mirrors_valid": True,
        "csr_shape_valid": True,
        "reverse_csr_valid": True,
        "orphan_node_slots": [],
        "multiply_identified_node_slots": [],
        "invalid_identity_ids": [],
        "out_of_range_identity_ids": [],
        "orphan_edge_slots": [],
        "resolution_inputs": [],
        "resolution_input_digest": digest(63),
        "resolution_hints": [],
        "resolution_hint_digest": digest(64),
        "resolution_decisions": [],
        "resolution_digest": digest(65),
        "pipeline_receipt": pipeline_receipt(spec, binary_digest),
        "pipeline_digest": preview["candidate_pipeline_digest"],
        "coverage": "COMPLETE",
        "unowned_nodes": [],
        "unowned_edges": [],
        "dangling_node_claims": [],
        "dangling_edge_claims": [],
        "duplicate_graph_edges": [],
        "lineage_digest": digest(66),
        "ownership_digest": preview["candidate_ownership_digest"],
    }
    recomputed = _recomputed_ownership_digests(manifest)
    manifest.update(recomputed)
    preview["candidate_pipeline_digest"] = recomputed["pipeline_digest"]
    preview["candidate_ownership_digest"] = recomputed["ownership_digest"]
    return manifest


def graph_ingest_result(spec: OwnerSpec, preview: dict, binary_digest: str) -> dict:
    count = len(spec.source_digests)
    manifest = ownership_manifest(spec, preview, binary_digest)
    return {
        "mode": "REPLACE",
        "root_identity": str(spec.root),
        "reconciliation_brain_id": preview["actor_brain_id"],
        "ownership_manifest": manifest,
        "parent": None,
        "candidate_ownership_digest": preview["candidate_ownership_digest"],
        "candidate_source_projection_digest": preview[
            "candidate_source_projection_digest"
        ],
        "candidate_pipeline_digest": preview["candidate_pipeline_digest"],
        "actor_checkpoint_required": True,
        "ingest_output": {
            "mode": "replace",
            "adapter": "code",
            "namespace": None,
            "files_scanned": count,
            "files_parsed": count,
            "nodes_created": 10,
            "edges_created": 12,
            "elapsed_ms": 1,
            "node_count": 10,
            "edge_count": 12,
            "light_evidence_resolved": 0,
            "light_evidence_unresolved": 0,
            "memory_freshness": {"stale_evidence_count": 0, "stale_evidence": []},
        },
        "graph_generation_before": preview["expected_graph_generation"],
        "graph_generation_after": preview["expected_graph_generation"] + 1,
        "checkpoint_ack": {
            "schema": CHECKPOINT_ACK_SCHEMA,
            "checkpoint_id": "checkpoint-1",
            "brain_id": preview["actor_brain_id"],
            "epoch": 1,
            "generation": preview["expected_graph_generation"] + 1,
            "revision": 1,
            "current_pointer_digest": digest(67),
            "confirmed_at_unix_ms": 1_700_000_000_001,
        },
    }


class RecordingProvider:
    def __init__(self, spec: OwnerSpec, authority: AuthorityAssembly) -> None:
        self.spec = spec
        self.authority = authority
        self.foreign_field: str | None = None
        self.requests: list[dict] = []

    @property
    def identity_digest(self) -> str:
        return self.authority.provider_executable_digest

    @property
    def blind_boundary_kind(self) -> str:
        return self.authority.blind_boundary_kind

    @property
    def blind_boundary_proven(self) -> bool:
        return self.authority.blind_boundary_proven

    def preflight(self, request: dict) -> dict:
        self.requests.append(request)
        return {
            "schema": AUTHORITY_PREFLIGHT_RESPONSE_SCHEMA,
            "request_id": request["request_id"],
            "provider_kind": self.authority.provider_kind,
            "production_authority_assembly": self.authority.production_authority_assembly,
            "assembly_id": self.authority.assembly_id,
            "assembly_digest": self.authority.assembly_digest,
            "binary_digest": request["binary_digest"],
            "provider_executable_digest": self.authority.provider_executable_digest,
        }

    def authorize(self, request: dict) -> dict:
        self.requests.append(request)
        preview = request["preview"]
        owner = request["owner"]
        binding = preview["authority_binding"]
        response = {
            "schema": AUTHORITY_PROVIDER_RESPONSE_SCHEMA,
            "request_id": request["request_id"],
            "provider_kind": self.authority.provider_kind,
            "assembly_id": self.authority.assembly_id,
            "assembly_digest": self.authority.assembly_digest,
            "owner_id": owner["owner_id"],
            "repo_id": owner["repo_id"],
            "scope": owner["scope"],
            "source_revision": owner["source_revision"],
            "file_set_digest": owner["file_set_digest"],
            "binary_digest": owner["binary_digest"],
            "preview_id": preview["preview_id"],
            "transport_session_id": preview["transport_session_id"],
            "ingress_context_digest": preview["ingress_context_digest"],
            "operation_object_digest": preview["operation_object_digest"],
            "authorization_request": {
                "schema": AUTHORITY_AUTHORIZE_REQUEST_SCHEMA,
                "request_id": "authorize-repo-1",
                "authority_session_id": "authority-session-1",
                "authority_session_context_digest": preview["ingress_context_digest"],
                "target_action": binding["target_action"],
                "payload_digest": binding["payload_digest"],
                "requested_effects": binding["requested_effects"],
                "mission_id": binding["mission_id"],
                "mission_head_id": binding["mission_head_id"],
                "input": {"authority": "positive_sovereign"},
            },
        }
        if self.foreign_field is not None:
            response[self.foreign_field] = "foreign-binding"
        return response


class RecordingClient:
    def __init__(
        self,
        spec: OwnerSpec,
        *,
        foreign_lease: bool = False,
        foreign_source: bool = False,
        production_receipt: bool = True,
        receipt_signature: str | None = None,
        mutation_tamper: tuple[str, ...] | None = None,
        mutation_value: object = "tampered",
        mutation_sidecar: bool = False,
        outcome_tamper: bool = False,
    ) -> None:
        self.spec = spec
        self.session_id = "mcp-session-owner-1"
        self.authorization_lease_id: str | None = None
        self.preview = preview_for(spec, self.session_id)
        self.foreign_lease = foreign_lease
        self.foreign_source = foreign_source
        self.production_receipt = production_receipt
        self.receipt_signature = receipt_signature
        self.mutation_tamper = mutation_tamper
        self.mutation_value = mutation_value
        self.mutation_sidecar = mutation_sidecar
        self.outcome_tamper = outcome_tamper
        self.binary_digest = sha_digest(42)
        ownership_manifest(self.spec, self.preview, self.binary_digest)
        self.calls: list[tuple[int, str, dict, str | None]] = []

    def bind_authorization_lease(self, lease_id: str) -> None:
        self.authorization_lease_id = lease_id

    def clear_authorization_lease(self) -> None:
        self.authorization_lease_id = None

    def call_tool(self, name: str, arguments: dict) -> dict:
        self.calls.append((id(self), name, arguments, self.authorization_lease_id))
        if name == "graph_ingest_preview":
            return self.preview
        if name == "authority_authorize":
            lease = digest(9)
            receipt = production_authorization_receipt(
                self.preview, production=self.production_receipt
            )
            if self.receipt_signature is not None:
                receipt["signature"] = self.receipt_signature
            return {
                "schema": AUTHORITY_AUTHORIZE_RESPONSE_SCHEMA,
                "request_id": arguments["request_id"],
                "authorization_lease_id": lease,
                "authorization_receipt": receipt,
                "expires_at": receipt["core"]["expires_at"],
            }
        if name == "external_mutation_service":
            lease = digest(11) if self.foreign_lease else digest(9)
            root = (
                str(self.spec.root / "foreign")
                if self.foreign_source
                else str(self.spec.root)
            )
            result = graph_ingest_result(self.spec, self.preview, self.binary_digest)
            result["root_identity"] = root
            if self.mutation_tamper is not None:
                target = result
                for component in self.mutation_tamper[:-1]:
                    target = target[component]
                target[self.mutation_tamper[-1]] = self.mutation_value
            if self.mutation_sidecar:
                result["operator_sidecar"] = "../operator-only/corpus.json"
            outcome_digest = _rust_domain_digest(
                GRAPH_INGEST_OUTCOME_DIGEST_DOMAIN,
                [
                    self.preview["operation_object_digest"],
                    result["mode"],
                    result["root_identity"],
                    result["ownership_manifest"]["ownership_digest"],
                    result["ownership_manifest"]["source_projection_digest"],
                    result["parent"],
                ],
            )
            if self.outcome_tamper:
                outcome_digest = digest(999)
            return {
                "schema": EXTERNAL_MUTATION_RESPONSE_SCHEMA,
                "request_id": arguments["request_id"],
                "semantic_action": self.preview["semantic_action"],
                "semantic_payload_digest": self.preview["semantic_payload_digest"],
                "operation_object_digest": self.preview["operation_object_digest"],
                "authorization_lease_id": lease,
                "authorization_reservation_id": "reservation-1",
                "journal_operation_id": "journal-1",
                "outcome_digest": outcome_digest,
                "graph_resync_required": False,
                "reconciliation_state": "RECONCILED",
                "result": result,
            }
        raise AssertionError(f"unexpected tool call: {name}")


class ExtractMeasurementTests(unittest.TestCase):
    def test_preserves_canonical_verdict_and_caps_unique_anchors(self) -> None:
        seek = {
            "results": [
                {"node_id": "n1"},
                {"node_id": "n1"},
                {"node_id": "n2"},
                {"node_id": "n3"},
                {"node_id": "n4"},
                {"node_id": "n5"},
                {"node_id": "n6"},
            ],
            "trust_envelope": {
                "verdict": "act",
                "calibrated": True,
                "calibration_receipt": wire_calibration_receipt(),
            },
        }
        row, raw, calibration = extract_measurement("t1", 1.25, 2.5, seek)
        self.assertEqual(raw, "act")
        self.assertEqual(row["verdict"], "act")
        self.assertEqual(row["ranked_anchor_ids"], ["n1", "n2", "n3", "n4", "n5"])
        self.assertIs(row["north_executed"], True)
        self.assertIs(row["seek_executed"], True)
        self.assertEqual(row["ranked_scores"], [])
        self.assertIn("sufficiency", row)
        self.assertIn("trust_envelope", row)
        self.assertEqual(
            row["trust_envelope"]["calibration_receipt_digest"],
            "sha256:" + wire_calibration_receipt()["receipt_digest"],
        )
        self.assertEqual(
            calibration["receipt_digest"],
            "sha256:" + wire_calibration_receipt()["receipt_digest"],
        )

    def test_missing_verdict_falls_back_without_synthesizing_act(self) -> None:
        row, raw, calibration = extract_measurement(
            "t1",
            1.0,
            2.0,
            {
                "results": [{"node_id": "n1"}],
                "trust_envelope": {"calibrated": False},
            },
        )
        self.assertIsNone(raw)
        self.assertIsNone(calibration)
        self.assertEqual(row["verdict"], "reverify")

    def test_zero_candidates_falls_back_to_abstain(self) -> None:
        row, raw, calibration = extract_measurement(
            "t1",
            1.0,
            2.0,
            {"results": [], "trust_envelope": {"calibrated": False}},
        )
        self.assertIsNone(raw)
        self.assertIsNone(calibration)
        self.assertEqual(row["verdict"], "abstain")
        self.assertEqual(row["ranked_anchor_ids"], [])

    def test_act_without_exact_calibration_receipt_is_rejected(self) -> None:
        with self.assertRaisesRegex(RunnerError, "act without a valid calibration"):
            extract_measurement(
                "t1",
                1.0,
                2.0,
                {
                    "results": [{"node_id": "n1"}],
                    "trust_envelope": {"verdict": "act", "calibrated": False},
                },
            )


class CalibrationReceiptTests(unittest.TestCase):
    def test_projects_exact_rust_raw_digest_to_prefixed_scorer_digest(self) -> None:
        wire = wire_calibration_receipt()
        projection = validate_calibration_receipt(wire)
        self.assertEqual(
            projection["receipt_digest"], "sha256:" + wire["receipt_digest"]
        )
        self.assertEqual(projection["receipt_schema"], SEEK_CALIBRATION_RECEIPT_SCHEMA)
        self.assertEqual(projection["measured_precision"], 0.995)

    def test_tampered_receipt_row_is_rejected(self) -> None:
        wire = wire_calibration_receipt()
        wire["coverage"] = 0.41
        with self.assertRaisesRegex(RunnerError, "does not bind its complete row"):
            validate_calibration_receipt(wire)


class RunnerCheckpointTests(unittest.TestCase):
    @staticmethod
    def checkpoint() -> dict:
        queries = {
            "corpus_id": "corpus-1",
            "corpus_digest": sha_digest(70),
            "self_digest": sha_digest(71),
            "tasks": [{"task_id": "task-1"}],
            "source_manifest": {
                "manifest_digest": sha_digest(72),
                "source_commit": "b" * 40,
            },
        }
        ingests = [
            {
                "repo_id": "repo-1",
                "owner_id": "owner-1",
                "source_revision": "revision-1",
                "file_set_digest": sha_digest(73),
                "mcp_session_id": "session-1",
                "candidate_ownership_digest": digest(74),
                "candidate_source_projection_digest": digest(75),
                "candidate_pipeline_digest": digest(76),
                "mutation_proof": {
                    "checkpoint_id": "checkpoint-1",
                    "checkpoint_generation": 2,
                    "current_pointer_digest": digest(77),
                },
            }
        ]
        return build_runner_checkpoint(
            queries=queries,
            lane="current",
            run_id="run-1",
            system_revision="system-1",
            sealed_corpus_self_digest=sha_digest(78),
            metric_spec_digest=sha_digest(79),
            runner_digest=sha_digest(80),
            binary_digest=sha_digest(81),
            governed_ingests=ingests,
            measurements=[{"task_id": "task-1"}],
        )

    def test_checkpoint_is_identity_bound_and_explicitly_not_resumable(self) -> None:
        checkpoint = self.checkpoint()
        self.assertIs(checkpoint["resume_supported"], False)
        self.assertEqual(
            checkpoint["owner_ingest_bindings"][0]["mcp_session_id"], "session-1"
        )
        self.assertIs(validate_runner_checkpoint(checkpoint), checkpoint)

    def test_checkpoint_tamper_and_resume_claim_are_rejected(self) -> None:
        checkpoint = self.checkpoint()
        checkpoint["owner_ingest_bindings"][0]["checkpoint_generation"] = 3
        with self.assertRaisesRegex(RunnerError, "identity/resume"):
            validate_runner_checkpoint(checkpoint)

        checkpoint = self.checkpoint()
        checkpoint["resume_supported"] = True
        checkpoint["self_digest"] = _self_digest(checkpoint)
        with self.assertRaisesRegex(RunnerError, "identity/resume"):
            validate_runner_checkpoint(checkpoint)

    def test_checkpoint_write_fsyncs_file_and_parent_directory(self) -> None:
        checkpoint = self.checkpoint()
        with tempfile.TemporaryDirectory() as temporary:
            path = pathlib.Path(temporary) / "checkpoint.json"
            with mock.patch("os.fsync", wraps=os.fsync) as fsync:
                _write_json_durable(path, checkpoint)
            observed = json.loads(path.read_text(encoding="utf-8"))
        self.assertEqual(observed["self_digest"], checkpoint["self_digest"])
        self.assertGreaterEqual(fsync.call_count, 2)


class ValidateArtifactTests(unittest.TestCase):
    @staticmethod
    def queries() -> dict:
        return {
            "corpus_id": "c1",
            "corpus_digest": sha_digest(201),
            "self_digest": sha_digest(202),
            "source_manifest": {
                "manifest_digest": sha_digest(203),
                "source_commit": "b" * 40,
            },
            "tasks": [{"task_id": "t1"}],
        }

    @classmethod
    def artifact(cls) -> dict:
        queries = cls.queries()
        artifact = {
            "schema": RESULT_SCHEMA,
            "lane": "current",
            "run_id": "run-1",
            "corpus_id": queries["corpus_id"],
            "corpus_digest": queries["corpus_digest"],
            "public_corpus_self_digest": queries["self_digest"],
            "sealed_corpus_self_digest": sha_digest(204),
            "source_manifest_digest": queries["source_manifest"]["manifest_digest"],
            "source_revision": queries["source_manifest"]["source_commit"],
            "system_revision": "r1",
            "binary_digest": sha_digest(205),
            "runner_digest": sha_digest(206),
            "metric_spec_digest": sha_digest(207),
            "measurements": [
                {
                    "task_id": "t1",
                    "ranked_anchor_ids": ["n1"],
                    "verdict": "reverify",
                    "north_latency_ms": 1.0,
                    "seek_latency_ms": 2.0,
                    "north_executed": True,
                    "seek_executed": True,
                    "ranked_scores": [0.5],
                    "relevance_clearing_total": 1.0,
                    "sufficiency": {
                        "state": "partial",
                        "captured": 1,
                        "marginal_score": 0.1,
                        "top_score": 0.5,
                    },
                    "trust_envelope": {
                        "calibrated": False,
                        "score": 0.5,
                        "verdict": "reverify",
                        "calibration_receipt_digest": None,
                    },
                }
            ],
            "run_metadata": {
                "schema": "m1nd10-g6-blind-run-metadata-v2",
                "lane": "current",
                "run_id": "run-1",
                "generated_at": "2026-07-19T00:00:00Z",
                "started_at": "2026-07-19T00:00:00Z",
                "transport": "mcp-http-loopback",
                "task_count": 1,
                "unscored": True,
                "score_eligible": False,
                "diagnostic_only": True,
                "proof_state": "NOT_PROVEN",
                "formal_preflights": {
                    "complete": False,
                    "status": "NOT_PROVEN",
                    "missing": ["production authority receipt/signer assembly"],
                    "delivery": "delivery-2-hardened-runner",
                    "same_session_readiness_ingest_measurement_delete": True,
                    "process_group_cleanup": True,
                    "source_live_identity": True,
                    "source_post_ingest_identity": True,
                    "authority_blind_boundary": {"kind": "test", "proven": True},
                    "owner_readiness_bindings_proven": True,
                    "path_topology": {
                        "absolute": True,
                        "fresh_mutable_roots": True,
                        "disjoint": True,
                        "symlink_free_path_components": True,
                        "paths": {"source_root": "/source"},
                    },
                    "authority_receipts_proven": False,
                    "checkpoint": {"enabled": False},
                },
                "authority_mode": "diagnostic_software_permitted",
                "authority_provider_kind": "software_test",
                "authority_provider_claimed_production_assembly": False,
                "production_authority_assembly_proven": False,
                "authority_assembly_id": "assembly-1",
                "authority_assembly_digest": digest(208),
                "authority_assembly_digest_verified": True,
                "authority_provider_executable_digest": sha_digest(209),
                "authority_owner_security_config_digest": sha_digest(210),
                "authority_key_registry_epoch": 1,
                "authority_receipt_key_id": "software-key-1",
                "authority_blind_boundary_kind": "test",
                "authority_blind_boundary_proven": True,
                "labels_read": False,
                "actions_executed": 0,
                "benchmark_task_actions_executed": 0,
                "governed_setup_mutations_executed": 1,
                "verdict_mapping": "canonical",
                "raw_runtime_verdict_counts": {},
                "calibration": {"status": "NOT_PROVEN"},
                "source_verification": {"exact_live_file_set": True},
                "post_ingest_source_verification": {"exact_live_file_set": True},
                "owner_topology": [
                    {
                        "repo_id": "repo-1",
                        "owner_id": "owner-1",
                        "instance_id": "instance-1",
                        "source_revision": "revision-1",
                        "file_set_digest": sha_digest(211),
                        "source_root": "/source/repo-1",
                        "port": 49_152,
                        "runtime_dir": "/runtime/repo-1",
                        "registry_dir": "/registry/repo-1",
                        "process_isolated": True,
                        "mcp_session_isolated": True,
                        "readiness": {
                            "pid": 42_424,
                            "started_at_ms": FIXTURE_NOW_MS,
                            "registry_entry_digest": sha_digest(212),
                            "manifest_digest": digest(213),
                            "binary_digest": sha_digest(205),
                            "token_captured_once": True,
                            "owner_binding_proven": True,
                        },
                        "mcp_session_id": "session-1",
                        "cleanup": {
                            "repo_id": "repo-1",
                            "same_session_for_owner_lifetime": True,
                            "session_delete_proven": True,
                            "process_group_terminated": True,
                            "cleanup_complete": True,
                        },
                    }
                ],
                "owner_cleanup": [
                    {
                        "repo_id": "repo-1",
                        "same_session_for_owner_lifetime": True,
                        "session_delete_proven": True,
                        "process_group_terminated": True,
                        "cleanup_complete": True,
                    }
                ],
                "governed_graph_ingest": [
                    {
                        "repo_id": "repo-1",
                        "owner_id": "owner-1",
                        "source_revision": "revision-1",
                        "file_set_digest": sha_digest(211),
                        "semantic_payload_digest": digest(214),
                        "operation_object_digest": digest(215),
                        "mcp_session_id": "session-1",
                        "candidate_ownership_digest": digest(216),
                        "candidate_source_projection_digest": digest(217),
                        "candidate_pipeline_digest": digest(218),
                        "authorization_lease_bound": True,
                        "authority_receipt": {
                            "authority_variant": None,
                            "control_verified_ed25519": False,
                            "receipt_core_digest_verified": True,
                            "assembly_digest_verified": True,
                            "key_registry_epoch": 1,
                            "signature_verified": False,
                            "clock_verified": True,
                            "key_lifecycle_verified": False,
                            "checked_at_ms": FIXTURE_NOW_MS,
                            "receipt_signer_metadata_production": False,
                            "production_authority_receipt_proven": False,
                            "receipt_digest": digest(219),
                            "issuer": "software-owner",
                            "key_id": "software-key-1",
                            "algorithm": "SOFTWARE_TEST_NOT_PROVEN",
                        },
                        "production_authority_receipt_proven": False,
                        "reconciliation_state": "RECONCILED",
                        "files_scanned": 1,
                        "files_parsed": 1,
                        "node_count": 1,
                        "edge_count": 0,
                        "mutation_proof": {},
                        "governed_ingest_latency_ms": 1.0,
                    }
                ],
                "warmup": {},
                "errors": [],
            },
            "self_digest": "",
        }
        seal(artifact)
        return artifact

    def test_accepts_exact_public_coverage(self) -> None:
        self.assertEqual(
            validate_unscored_artifact(self.artifact(), self.queries()), []
        )

    def test_rejects_duplicate_or_overlong_anchors(self) -> None:
        queries = self.queries()
        artifact = self.artifact()
        artifact["measurements"][0]["ranked_anchor_ids"] = [
            "n1",
            "n1",
            "n2",
            "n3",
            "n4",
            "n5",
        ]
        seal(artifact)
        self.assertIn(
            "t1: invalid ranked anchors", validate_unscored_artifact(artifact, queries)
        )

    def test_rejects_unexecuted_measurement(self) -> None:
        queries = self.queries()
        artifact = self.artifact()
        artifact["measurements"][0]["seek_executed"] = False
        seal(artifact)
        errors = validate_unscored_artifact(artifact, queries)
        self.assertIn("t1: seek call was not executed", errors)

    def test_rejects_unknown_fields_provenance_drift_and_nonfinite_nested_score(
        self,
    ) -> None:
        artifact = self.artifact()
        artifact["measurements"][0]["operator_sidecar"] = "../labels.json"
        artifact["source_manifest_digest"] = sha_digest(999)
        artifact["measurements"][0]["trust_envelope"]["score"] = float("nan")
        seal(artifact)
        errors = validate_unscored_artifact(artifact, self.queries())
        self.assertIn("t1: closed field set mismatch", errors)
        self.assertIn("source_manifest_digest mismatch", errors)
        self.assertIn("t1: invalid trust score", errors)

    def test_rederives_formal_authority_readiness_and_cleanup_instead_of_trusting_summary(
        self,
    ) -> None:
        cases = []
        artifact = self.artifact()
        artifact["run_metadata"]["formal_preflights"]["complete"] = True
        artifact["run_metadata"]["formal_preflights"]["status"] = "PROVEN"
        artifact["run_metadata"]["formal_preflights"]["missing"] = []
        cases.append((artifact, "formal preflight summary"))

        artifact = self.artifact()
        artifact["run_metadata"]["owner_cleanup"][0]["cleanup_complete"] = False
        cases.append((artifact, "cleanup is incomplete"))

        artifact = self.artifact()
        artifact["run_metadata"]["owner_topology"][0]["readiness"]["binary_digest"] = (
            sha_digest(999)
        )
        cases.append((artifact, "owner readiness binding is incomplete"))

        for artifact, message in cases:
            with self.subTest(message=message):
                seal(artifact)
                self.assertTrue(
                    any(
                        message in error
                        for error in validate_unscored_artifact(
                            artifact, self.queries()
                        )
                    )
                )


class ValidatePublicQueriesTests(unittest.TestCase):
    @staticmethod
    def real_public() -> dict:
        return json.loads(REAL_PUBLIC.read_text(encoding="utf-8"))

    def test_accepts_actual_held_out_v2_public_artifact(self) -> None:
        artifact = self.real_public()
        tasks = validate_public_queries(artifact)
        self.assertEqual(artifact["schema"], PUBLIC_SCHEMA)
        self.assertEqual(len(tasks), 220)
        self.assertEqual(artifact["task_count"], 220)

    def test_rejects_historical_v1_explicitly(self) -> None:
        artifact = self.real_public()
        artifact["schema"] = HISTORICAL_PUBLIC_SCHEMA
        with self.assertRaisesRegex(RunnerError, "historical held-out-v1"):
            validate_public_queries(artifact)

    def test_rejects_label_bearing_task_even_when_resealed(self) -> None:
        artifact = self.real_public()
        artifact["tasks"][0]["localizable"] = True
        seal(artifact)
        with self.assertRaisesRegex(RunnerError, "label-bearing"):
            validate_public_queries(artifact)

    def test_rejects_tampered_public_self_digest(self) -> None:
        artifact = self.real_public()
        artifact["tasks"][0]["query"] += "?"
        with self.assertRaisesRegex(RunnerError, "self_digest mismatch"):
            validate_public_queries(artifact)

    def test_rejects_resealed_manifest_digest_tamper(self) -> None:
        artifact = self.real_public()
        artifact["source_manifest"]["manifest_digest"] = sha_digest(999)
        seal(artifact)
        with self.assertRaisesRegex(RunnerError, "source manifest digest mismatch"):
            validate_public_queries(artifact)

    def test_rejects_public_sidecar_field_even_when_resealed(self) -> None:
        artifact = self.real_public()
        artifact["digests_sidecar"] = "../operator-only/digests.json"
        seal(artifact)
        with self.assertRaisesRegex(RunnerError, "closed JSON field set"):
            validate_public_queries(artifact)

    def test_rejects_manifest_backslash_or_parent_traversal_when_resealed(self) -> None:
        for hostile in ("../operator-only", "sources\\foreign"):
            with self.subTest(hostile=hostile):
                artifact = self.real_public()
                artifact["source_manifest"]["repos"][0]["source_root"] = hostile
                manifest = artifact["source_manifest"]
                manifest["manifest_digest"] = _sha256_bytes(
                    _canonical_bytes(_without_key(manifest, "manifest_digest"))
                )
                seal(artifact)
                with self.assertRaisesRegex(
                    RunnerError, "escapes|canonical POSIX relative path"
                ):
                    validate_public_queries(artifact)

    def test_rejects_runner_contract_operator_path_tamper(self) -> None:
        artifact = self.real_public()
        artifact["runner_contract"]["forbidden_artifact"] = (
            "../operator-only/corpus.json"
        )
        seal(artifact)
        with self.assertRaisesRegex(RunnerError, "exposes labels"):
            validate_public_queries(artifact)

    def test_accepts_commit_or_snapshot_digest_as_source_identity(self) -> None:
        self.assertEqual(
            public_source_revision({"source_manifest": {"source_commit": "abc123"}}),
            "abc123",
        )
        self.assertEqual(
            public_source_revision(
                {"source_manifest": {"snapshot_digest": "sha256:immutable"}}
            ),
            "sha256:immutable",
        )

    def test_rejects_missing_source_identity_before_runtime(self) -> None:
        with self.assertRaisesRegex(RunnerError, "immutable source identity"):
            public_source_revision(
                {"source_manifest": {"snapshot_id": "mutable-label"}}
            )


class OwnerTopologyTests(unittest.TestCase):
    def test_four_manifest_repositories_map_to_four_isolated_owners(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            queries = {
                "source_manifest": {
                    "repos": [
                        {
                            "repo_id": f"repo-{index}",
                            "source_root": f"sources/repo-{index}",
                            "source_revision": f"revision-{index}",
                            "file_set_digest": sha_digest(index),
                        }
                        for index in range(1, 5)
                    ]
                },
                "tasks": [
                    {
                        "task_id": f"t-{index}",
                        "repo_id": f"repo-{index}",
                        "repo_revision": f"revision-{index}",
                    }
                    for index in range(1, 5)
                ],
            }
            specs = build_owner_specs(
                queries,
                root / "source",
                root / "runtime",
                root / "registry",
                18200,
            )

        self.assertEqual(
            [spec.repo_id for spec in specs], [f"repo-{i}" for i in range(1, 5)]
        )
        self.assertEqual([spec.port for spec in specs], [0, 0, 0, 0])
        self.assertEqual(len({spec.owner_id for spec in specs}), 4)
        self.assertEqual(len({spec.root for spec in specs}), 4)
        self.assertEqual(len({spec.runtime_dir for spec in specs}), 4)
        self.assertEqual(len({spec.registry_dir for spec in specs}), 4)
        self.assertTrue(all(spec.scope.endswith(str(spec.root)) for spec in specs))

    def test_task_revision_must_match_manifest_source_binding(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            queries = {
                "source_manifest": {
                    "repos": [
                        {
                            "repo_id": "repo-1",
                            "source_root": "repo-1",
                            "source_revision": "revision-1",
                            "file_set_digest": sha_digest(1),
                        }
                    ]
                },
                "tasks": [
                    {
                        "task_id": "t-1",
                        "repo_id": "repo-1",
                        "repo_revision": "foreign-revision",
                    }
                ],
            }
            with self.assertRaisesRegex(RunnerError, "task revision"):
                build_owner_specs(
                    queries,
                    root / "source",
                    root / "runtime",
                    root / "registry",
                    18200,
                )


class OwnerReadinessBindingTests(unittest.TestCase):
    @staticmethod
    def fixture(root: pathlib.Path) -> tuple[OwnerSpec, dict, pathlib.Path]:
        spec = owner_spec(root)
        spec = OwnerSpec(**{**spec.__dict__, "port": 49_152})
        entry = {
            "instance_id": "instance-exact-1",
            "pid": 42_424,
            "started_at_ms": FIXTURE_NOW_MS,
            "workspace_root": str(spec.root),
            "runtime_root": str(spec.runtime_dir),
            "graph_source": str(spec.runtime_dir / "graph.json"),
            "bind": "127.0.0.1",
            "port": spec.port,
            "mode": "read_write",
            "status": "running",
            "owner_live": True,
            "stale": False,
            "conflicts": [],
        }
        return spec, entry, spec.registry_dir / "instances" / "instance-exact-1.json"

    def test_registry_and_authenticated_manifest_bind_exact_spawned_owner(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            spec, entry, entry_path = self.fixture(pathlib.Path(temporary).resolve())
            observed = _validate_registry_entry(
                entry,
                entry_path,
                process_pid=entry["pid"],
                spec=spec,
                launched_at_ms=entry["started_at_ms"],
            )
            instance_response = {
                "instance": {
                    key: entry[key]
                    for key in (
                        "instance_id",
                        "pid",
                        "started_at_ms",
                        "workspace_root",
                        "runtime_root",
                        "graph_source",
                        "bind",
                        "port",
                        "mode",
                        "status",
                    )
                },
                "graph_state": {
                    "runtime_root": str(spec.runtime_dir),
                    "workspace_root": str(spec.root),
                    "workspace_root_source": "env:M1ND_WORKSPACE_ROOT",
                },
            }
            manifest = {
                "schema": "m1nd-organism-manifest-v1",
                "runtime": {
                    "owner_id": entry["instance_id"],
                    "binary_version": "1.4.0",
                    "binary_sha256": sha_digest(42),
                    "started_at": entry["started_at_ms"],
                },
                "manifest_sha256": "",
            }
            manifest["manifest_sha256"] = _rust_domain_digest(
                "m1nd-organism-manifest-v1",
                _without_key(manifest, "manifest_sha256"),
            )
            receipt = _validate_owner_attestation(
                spec,
                observed,
                instance_response,
                {
                    "schema": "m1nd-organism-manifest-response-v1",
                    "manifest": manifest,
                    "verification": {
                        "computed_manifest_sha256": manifest["manifest_sha256"]
                    },
                },
                binary_digest=sha_digest(42),
            )
        self.assertIs(receipt.owner_binding_proven, True)
        self.assertEqual(receipt.pid, 42_424)
        self.assertNotEqual(receipt.port, 1338)

    def test_registry_rejects_foreign_pid_and_installed_owner_port(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            spec, entry, entry_path = self.fixture(pathlib.Path(temporary).resolve())
            for field, value, message in (
                ("pid", 42_425, "foreign PID"),
                ("port", 1338, "invalid or forbidden"),
            ):
                hostile = dict(entry)
                hostile[field] = value
                with (
                    self.subTest(field=field),
                    self.assertRaisesRegex(RunnerError, message),
                ):
                    _validate_registry_entry(
                        hostile,
                        entry_path,
                        process_pid=42_424,
                        spec=spec,
                        launched_at_ms=entry["started_at_ms"],
                    )


class SourceSnapshotTests(unittest.TestCase):
    @staticmethod
    def make_snapshot(root: pathlib.Path) -> dict:
        repo_root = root / "sources" / "repo-1"
        (repo_root / "src").mkdir(parents=True)
        contents = {
            "src/lib.rs": b"pub fn sealed() {}\n",
            "Cargo.toml": b"[package]\nname='sealed'\n",
        }
        files = []
        for relative, content in contents.items():
            path = repo_root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
            files.append(
                {
                    "path": relative,
                    "role": "source"
                    if relative.endswith(".rs")
                    else "dependency_manifest",
                    "bytes": len(content),
                    "lines": content.count(b"\n"),
                    "sha256": _sha256_bytes(content),
                }
            )
        return {
            "source_manifest": {
                "repos": [
                    {
                        "repo_id": "repo-1",
                        "source_root": "sources/repo-1",
                        "files": files,
                    }
                ]
            }
        }

    def test_live_snapshot_proves_exact_bytes_lines_sha_and_file_set(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary).resolve()
            queries = self.make_snapshot(root)
            proof = verify_public_source_snapshot(queries, root)
        self.assertEqual(proof["checked_files"], 2)
        self.assertEqual(proof["extra_files"], 0)
        self.assertIs(proof["exact_live_file_set"], True)
        self.assertIs(proof["git_objects_used_as_live_root"], False)

    def test_extra_dirty_or_drifted_live_file_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary).resolve()
            queries = self.make_snapshot(root)
            repo = root / "sources" / "repo-1"
            (repo / "src" / "dirty.rs").write_text("fn dirty() {}\n", encoding="utf-8")
            with self.assertRaisesRegex(RunnerError, "extra=1"):
                verify_public_source_snapshot(queries, root)

    def test_symlink_and_git_worktree_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary).resolve()
            queries = self.make_snapshot(root)
            repo = root / "sources" / "repo-1"
            (repo / "linked.rs").symlink_to(repo / "src" / "lib.rs")
            with self.assertRaisesRegex(RunnerError, "symlink"):
                verify_public_source_snapshot(queries, root)

        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary).resolve()
            queries = self.make_snapshot(root)
            (root / "sources" / "repo-1" / ".git").mkdir()
            with self.assertRaisesRegex(RunnerError, "worktree"):
                verify_public_source_snapshot(queries, root)


class RunnerPathTopologyTests(unittest.TestCase):
    @staticmethod
    def args(root: pathlib.Path):
        root.mkdir(parents=True, exist_ok=True)
        source = root / "source"
        source.mkdir()
        queries = root / "queries.json"
        metric = root / "metric.json"
        binary = root / "m1nd-mcp"
        for path in (queries, metric, binary):
            path.write_text("{}", encoding="utf-8")
        binary.chmod(0o700)
        return mock.Mock(
            queries=queries,
            metric_spec=metric,
            binary=binary,
            authority_provider=None,
            authority_assembly=None,
            source_root=source,
            runtime_dir=root / "runtime",
            registry_dir=root / "registry",
            output=root / "output.json",
            checkpoint=root / "checkpoint.json",
        )

    def test_accepts_absolute_fresh_disjoint_symlink_free_topology(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            args = self.args(pathlib.Path(temporary).resolve())
            proof = validate_runner_paths(args)
        self.assertIs(proof["absolute"], True)
        self.assertIs(proof["fresh_mutable_roots"], True)
        self.assertIs(proof["disjoint"], True)

    def test_rejects_relative_existing_and_overlapping_mutable_roots(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary).resolve()
            args = self.args(root)
            args.output = pathlib.Path("relative.json")
            with self.assertRaisesRegex(RunnerError, "output must be absolute"):
                validate_runner_paths(args)

            args = self.args(root / "second")
            args.runtime_dir.mkdir()
            with self.assertRaisesRegex(RunnerError, "fresh and absent"):
                validate_runner_paths(args)

            args = self.args(root / "third")
            args.runtime_dir = args.source_root / "runtime"
            with self.assertRaisesRegex(RunnerError, "overlap"):
                validate_runner_paths(args)

    def test_rejects_symlink_component(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary).resolve()
            args = self.args(root)
            real = root / "real-output"
            real.mkdir()
            linked = root / "linked-output"
            linked.symlink_to(real, target_is_directory=True)
            args.output = linked / "result.json"
            with self.assertRaisesRegex(RunnerError, "symlink component"):
                validate_runner_paths(args)


class TransportLifecycleTests(unittest.TestCase):
    def test_bearer_capture_is_private_canonical_and_secret_safe(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary).resolve()
            root.chmod(0o700)
            token_path = root / "token"
            token_path.write_text("ab" * 32 + "\n", encoding="ascii")
            token_path.chmod(0o600)
            captured = capture_private_bearer(token_path)

        self.assertEqual(captured.value, "ab" * 32)
        self.assertNotIn("ab" * 32, repr(captured))

    def test_bearer_rejects_symlink_public_mode_and_noncanonical_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary).resolve()
            root.chmod(0o700)
            target = root / "target"
            target.write_text("ab" * 32 + "\n", encoding="ascii")
            target.chmod(0o600)
            linked = root / "linked"
            linked.symlink_to(target)
            with self.assertRaisesRegex(RunnerError, "regular no-follow"):
                capture_private_bearer(linked)

            target.chmod(0o644)
            with self.assertRaisesRegex(RunnerError, "owner/mode/link/size"):
                capture_private_bearer(target)

            target.write_text("AB" * 32 + "\n", encoding="ascii")
            target.chmod(0o600)
            with self.assertRaisesRegex(RunnerError, "canonical lowercase hex"):
                capture_private_bearer(target)

            target.write_text("ab" * 33, encoding="ascii")
            with self.assertRaisesRegex(RunnerError, "owner/mode/link/size"):
                capture_private_bearer(target)

    def test_bearer_path_replacement_is_detected_before_http(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary).resolve()
            root.chmod(0o700)
            token_path = root / "token"
            token_path.write_text("ab" * 32 + "\n", encoding="ascii")
            token_path.chmod(0o600)
            captured = capture_private_bearer(token_path)
            token_path.unlink()
            token_path.write_text("cd" * 32 + "\n", encoding="ascii")
            token_path.chmod(0o600)
            client = McpHttpClient(
                "http://127.0.0.1:18000", root, captured, "test-client"
            )
            with self.assertRaisesRegex(RunnerError, "changed after capture"):
                client._headers()

    def test_delete_targets_the_exact_initialized_mcp_session(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary).resolve()
            root.chmod(0o700)
            token_path = root / "token"
            token_path.write_text("ab" * 32 + "\n", encoding="ascii")
            token_path.chmod(0o600)
            client = McpHttpClient(
                "http://127.0.0.1:18000",
                root,
                capture_private_bearer(token_path),
                "test-client",
            )
            client.session_id = "session-exact-1"
            response = mock.MagicMock(status=204)
            response.getcode.return_value = 204
            response.read.return_value = b""
            opened = mock.MagicMock()
            opened.return_value.__enter__.return_value = response
            with mock.patch("urllib.request.urlopen", opened):
                proof = client.delete_session()

        request = opened.call_args.args[0]
        self.assertEqual(request.get_method(), "DELETE")
        self.assertEqual(
            dict(request.header_items())["Mcp-session-id"], "session-exact-1"
        )
        self.assertIs(proof["session_delete_proven"], True)
        self.assertIsNone(client.session_id)

    def test_process_group_creation_is_explicit_on_posix_and_windows(self) -> None:
        self.assertEqual(
            _process_group_popen_kwargs("posix"), {"start_new_session": True}
        )
        self.assertIn("creationflags", _process_group_popen_kwargs("nt"))


class GovernedIngestTests(unittest.TestCase):
    def test_exact_three_tool_sequence_uses_one_session_and_never_generic_ingest(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            spec = owner_spec(pathlib.Path(temporary))
            authority = assembly()
            provider = RecordingProvider(spec, authority)
            client = RecordingClient(spec)
            summary = execute_governed_graph_ingest(
                spec,
                client,  # type: ignore[arg-type]
                provider,
                authority,
                RecordingVerifier(),
                diagnostic=False,
                lane="current",
                binary_digest=authority.binary_digest,
            )

        names = [call[1] for call in client.calls]
        self.assertEqual(
            names,
            [
                "graph_ingest_preview",
                "authority_authorize",
                "external_mutation_service",
            ],
        )
        self.assertNotIn("ingest", names)
        self.assertEqual(len({call[0] for call in client.calls}), 1)
        self.assertIsNone(client.calls[0][3])
        self.assertIsNone(client.calls[1][3])
        self.assertEqual(client.calls[2][3], digest(9))
        self.assertIsNone(client.authorization_lease_id)
        self.assertEqual(summary["reconciliation_state"], "RECONCILED")
        self.assertIs(summary["authorization_lease_bound"], True)

    def test_foreign_provider_owner_binding_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            spec = owner_spec(pathlib.Path(temporary))
            authority = assembly()
            provider = RecordingProvider(spec, authority)
            provider.foreign_field = "owner_id"
            with self.assertRaisesRegex(RunnerError, "foreign owner_id"):
                execute_governed_graph_ingest(
                    spec,
                    RecordingClient(spec),  # type: ignore[arg-type]
                    provider,
                    authority,
                    RecordingVerifier(),
                    diagnostic=False,
                    lane="current",
                    binary_digest=authority.binary_digest,
                )

    def test_foreign_provider_source_binding_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            spec = owner_spec(pathlib.Path(temporary))
            authority = assembly()
            provider = RecordingProvider(spec, authority)
            provider.foreign_field = "source_revision"
            with self.assertRaisesRegex(RunnerError, "foreign source_revision"):
                execute_governed_graph_ingest(
                    spec,
                    RecordingClient(spec),  # type: ignore[arg-type]
                    provider,
                    authority,
                    RecordingVerifier(),
                    diagnostic=False,
                    lane="current",
                    binary_digest=authority.binary_digest,
                )

    def test_foreign_lease_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            spec = owner_spec(pathlib.Path(temporary))
            authority = assembly()
            provider = RecordingProvider(spec, authority)
            with self.assertRaisesRegex(RunnerError, "foreign digest or lease"):
                execute_governed_graph_ingest(
                    spec,
                    RecordingClient(spec, foreign_lease=True),  # type: ignore[arg-type]
                    provider,
                    authority,
                    RecordingVerifier(),
                    diagnostic=False,
                    lane="current",
                    binary_digest=authority.binary_digest,
                )

    def test_foreign_runtime_source_projection_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            spec = owner_spec(pathlib.Path(temporary))
            authority = assembly()
            provider = RecordingProvider(spec, authority)
            with self.assertRaisesRegex(
                RunnerError, "foreign owner or source projection"
            ):
                execute_governed_graph_ingest(
                    spec,
                    RecordingClient(spec, foreign_source=True),  # type: ignore[arg-type]
                    provider,
                    authority,
                    RecordingVerifier(),
                    diagnostic=False,
                    lane="current",
                    binary_digest=authority.binary_digest,
                )

    def test_formal_run_rejects_software_test_receipt_despite_provider_claim(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            spec = owner_spec(pathlib.Path(temporary))
            authority = assembly()
            provider = RecordingProvider(spec, authority)
            with self.assertRaisesRegex(RunnerError, "production authority receipt"):
                execute_governed_graph_ingest(
                    spec,
                    RecordingClient(spec, production_receipt=False),  # type: ignore[arg-type]
                    provider,
                    authority,
                    RecordingVerifier(),
                    diagnostic=False,
                    lane="current",
                    binary_digest=authority.binary_digest,
                )

    def test_diagnostic_software_receipt_remains_explicitly_unproven(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            spec = owner_spec(pathlib.Path(temporary))
            authority = assembly(production=False)
            provider = RecordingProvider(spec, authority)
            summary = execute_governed_graph_ingest(
                spec,
                RecordingClient(spec, production_receipt=False),  # type: ignore[arg-type]
                provider,
                authority,
                None,
                diagnostic=True,
                lane="current",
                binary_digest=authority.binary_digest,
            )
        self.assertIs(summary["production_authority_receipt_proven"], False)

    def test_owner_source_digest_map_tamper_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            spec = owner_spec(pathlib.Path(temporary))
            authority = assembly()
            provider = RecordingProvider(spec, authority)
            with self.assertRaisesRegex(RunnerError, "exact live source candidate"):
                execute_governed_graph_ingest(
                    spec,
                    RecordingClient(
                        spec,
                        mutation_tamper=("ownership_manifest", "source_digests"),
                    ),  # type: ignore[arg-type]
                    provider,
                    authority,
                    RecordingVerifier(),
                    diagnostic=False,
                    lane="current",
                    binary_digest=authority.binary_digest,
                )

    def test_candidate_and_checkpoint_generation_tamper_are_rejected(self) -> None:
        cases = (
            (("candidate_pipeline_digest",), "foreign owner or source projection"),
            (("checkpoint_ack", "generation"), "checkpoint generation"),
            (("graph_generation_after",), "graph generation after"),
        )
        for path, message in cases:
            with self.subTest(path=path), tempfile.TemporaryDirectory() as temporary:
                spec = owner_spec(pathlib.Path(temporary))
                authority = assembly()
                provider = RecordingProvider(spec, authority)
                with self.assertRaisesRegex(RunnerError, message):
                    execute_governed_graph_ingest(
                        spec,
                        RecordingClient(spec, mutation_tamper=path),  # type: ignore[arg-type]
                        provider,
                        authority,
                        RecordingVerifier(),
                        diagnostic=False,
                        lane="current",
                        binary_digest=authority.binary_digest,
                    )

    def test_result_sidecar_or_unknown_field_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            spec = owner_spec(pathlib.Path(temporary))
            authority = assembly()
            provider = RecordingProvider(spec, authority)
            with self.assertRaisesRegex(RunnerError, "closed JSON field set"):
                execute_governed_graph_ingest(
                    spec,
                    RecordingClient(spec, mutation_sidecar=True),  # type: ignore[arg-type]
                    provider,
                    authority,
                    RecordingVerifier(),
                    diagnostic=False,
                    lane="current",
                    binary_digest=authority.binary_digest,
                )

    def test_formal_run_rejects_valid_length_forged_signature(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            spec = owner_spec(pathlib.Path(temporary))
            authority = assembly()
            with self.assertRaisesRegex(RunnerError, "cryptographic verification"):
                execute_governed_graph_ingest(
                    spec,
                    RecordingClient(spec, receipt_signature="03" * 64),  # type: ignore[arg-type]
                    RecordingProvider(spec, authority),
                    authority,
                    RecordingVerifier(),
                    diagnostic=False,
                    lane="current",
                    binary_digest=authority.binary_digest,
                )

    def test_typed_claim_pipeline_resolution_and_outcome_tamper_are_rejected(
        self,
    ) -> None:
        class ResolutionTamperClient(RecordingClient):
            def call_tool(self, name: str, arguments: dict) -> dict:
                response = super().call_tool(name, arguments)
                if name == "external_mutation_service":
                    ownership = response["result"]["ownership_manifest"]
                    ownership["claims_by_source"]["src/a.rs"]["node_ids"] = ["node-1"]
                    ownership["resolution_inputs"] = [
                        {
                            "source_key": "src/a.rs",
                            "source_id": "node-1",
                            "target_label": "Target",
                            "relation": "calls",
                        }
                    ]
                    ownership["resolution_decisions"] = [
                        {
                            "source_key": "src/a.rs",
                            "source_id": "node-1",
                            "target_label": "Target",
                            "relation": "calls",
                            "outcome": "UNRESOLVED",
                            "resolved_target_id": None,
                            "candidate_ids": [],
                            "source_line_start": None,
                            "source_line_end": None,
                        }
                    ]
                return response

        cases = (
            (
                RecordingClient(
                    owner_spec(pathlib.Path("/tmp")),
                    mutation_tamper=(
                        "ownership_manifest",
                        "claims_by_source",
                        "src/a.rs",
                        "node_ids",
                    ),
                    mutation_value=["node-1"],
                ),
                "ownership_digest does not bind",
            ),
            (
                RecordingClient(
                    owner_spec(pathlib.Path("/tmp")),
                    mutation_tamper=(
                        "ownership_manifest",
                        "pipeline_receipt",
                        "policy_fingerprint",
                    ),
                    mutation_value=digest(998),
                ),
                "pipeline digest does not bind",
            ),
            (
                ResolutionTamperClient(owner_spec(pathlib.Path("/tmp"))),
                "resolution_input_digest does not bind",
            ),
            (
                RecordingClient(owner_spec(pathlib.Path("/tmp")), outcome_tamper=True),
                "outcome digest does not bind",
            ),
        )
        for prototype, message in cases:
            with (
                self.subTest(message=message),
                tempfile.TemporaryDirectory() as temporary,
            ):
                spec = owner_spec(pathlib.Path(temporary))
                client = type(prototype)(spec)
                if type(prototype) is RecordingClient:
                    client = RecordingClient(
                        spec,
                        mutation_tamper=prototype.mutation_tamper,
                        mutation_value=prototype.mutation_value,
                        outcome_tamper=prototype.outcome_tamper,
                    )
                authority = assembly()
                with self.assertRaisesRegex(RunnerError, message):
                    execute_governed_graph_ingest(
                        spec,
                        client,  # type: ignore[arg-type]
                        RecordingProvider(spec, authority),
                        authority,
                        RecordingVerifier(),
                        diagnostic=False,
                        lane="current",
                        binary_digest=authority.binary_digest,
                    )


class AuthorityAssemblyTests(unittest.TestCase):
    def test_loads_only_exact_independently_pinned_active_assembly(self) -> None:
        document = assembly_document()
        observed = load_authority_assembly(
            document,
            expected_digest=document["self_digest"],
            binary_digest=document["owner_binary_digest"],
            provider_executable_digest=document["provider_executable_digest"],
            blind_boundary_kind="darwin-sandbox-exec-deny-default-v1",
            blind_boundary_proven=True,
            now_ms=FIXTURE_NOW_MS,
        )
        self.assertIs(observed.expected_digest_verified, True)
        self.assertEqual(observed.verification_key.key_id, "owner-key-1")

    def test_rejects_digest_binary_provider_and_key_lifecycle_drift(self) -> None:
        cases = []
        document = assembly_document()
        cases.append((document, digest(999), sha_digest(42), sha_digest(43), "pinned"))
        document = assembly_document()
        cases.append(
            (
                document,
                document["self_digest"],
                sha_digest(999),
                sha_digest(43),
                "foreign owner",
            )
        )
        document = assembly_document()
        cases.append(
            (
                document,
                document["self_digest"],
                sha_digest(42),
                sha_digest(999),
                "foreign provider",
            )
        )
        document = assembly_document()
        document["verification_key_registry"]["keys"]["owner-key-1"]["status"] = (
            "REVOKED"
        )
        document["verification_key_registry"]["keys"]["owner-key-1"]["revoked_at"] = (
            FIXTURE_NOW_MS - 1
        )
        document["self_digest"] = _authority_assembly_digest(document)
        cases.append(
            (
                document,
                document["self_digest"],
                sha_digest(42),
                sha_digest(43),
                "not ACTIVE",
            )
        )
        for document, expected, owner_digest, provider_digest, message in cases:
            with (
                self.subTest(message=message),
                self.assertRaisesRegex(RunnerError, message),
            ):
                load_authority_assembly(
                    document,
                    expected_digest=expected,
                    binary_digest=owner_digest,
                    provider_executable_digest=provider_digest,
                    blind_boundary_kind="test",
                    blind_boundary_proven=True,
                    now_ms=FIXTURE_NOW_MS,
                )


class AuthorityProviderTests(unittest.TestCase):
    def test_formal_run_fails_closed_when_provider_is_missing(self) -> None:
        authority = assembly()
        with self.assertRaisesRegex(RunnerError, "required for formal"):
            preflight_authority_provider(
                None,
                authority,
                diagnostic=False,
                lane="current",
                binary_digest=authority.binary_digest,
            )

    def test_preflight_claim_is_not_treated_as_production_proof(self) -> None:
        authority = assembly(production=False)
        provider = RecordingProvider(
            owner_spec(pathlib.Path("/tmp/m1nd10-provider-test")), authority
        )
        observed = preflight_authority_provider(
            provider,
            authority,
            diagnostic=True,
            lane="current",
            binary_digest=authority.binary_digest,
        )
        self.assertIs(observed.production_authority_assembly, False)

    def test_authority_metadata_does_not_claim_score_eligibility(self) -> None:
        metadata = authority_run_metadata(
            diagnostic=True, assembly=assembly(production=False)
        )
        self.assertNotIn("score_eligible", metadata)
        self.assertNotIn("diagnostic_only", metadata)
        self.assertNotIn("proof_state", metadata)
        self.assertEqual(metadata["authority_provider_kind"], "software_test")
        self.assertIs(metadata["authority_provider_claimed_production_assembly"], False)
        self.assertIs(metadata["production_authority_assembly_proven"], False)

    def test_provider_uses_minimal_env_stdin_only_and_does_not_echo_stderr(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            executable = pathlib.Path(temporary).resolve() / "provider"
            executable.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "request = json.load(sys.stdin)\n"
                "print(json.dumps({'argv': sys.argv, 'request': request, "
                "'secret': os.environ.get('SUPER_SECRET')}))\n",
                encoding="utf-8",
            )
            executable.chmod(0o700)
            provider = ExternalAuthorityProvider(executable)
            with mock.patch.dict(os.environ, {"SUPER_SECRET": "authority-material"}):
                response = provider.preflight({"schema": "request", "request_id": "r1"})
            self.assertEqual(
                pathlib.Path(response["argv"][0]).name, "authority-provider"
            )
            self.assertEqual(response["request"]["request_id"], "r1")
            self.assertIsNone(response["secret"])

            executable.write_text(
                "#!/usr/bin/env python3\n"
                "import sys\n"
                "sys.stderr.write('SUPER_SECRET_AUTHORITY_MATERIAL')\n"
                "raise SystemExit(1)\n",
                encoding="utf-8",
            )
            provider = ExternalAuthorityProvider(executable)
            with self.assertRaises(RunnerError) as raised:
                provider.preflight({"schema": "request", "request_id": "r2"})
            self.assertNotIn("SUPER_SECRET_AUTHORITY_MATERIAL", str(raised.exception))

    def test_provider_timeout_and_live_output_bound_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            executable = pathlib.Path(temporary).resolve() / "provider"
            executable.write_text(
                "#!/usr/bin/env python3\nimport time\ntime.sleep(5)\n",
                encoding="utf-8",
            )
            executable.chmod(0o700)
            with self.assertRaisesRegex(RunnerError, "timed out"):
                ExternalAuthorityProvider(executable, timeout=0.05).preflight({})

            executable.write_text(
                "#!/usr/bin/env python3\nimport sys\n"
                "sys.stdout.write('x' * (1024 * 1024 + 1))\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(RunnerError, "bounded limit"):
                ExternalAuthorityProvider(executable, timeout=3).preflight({})

    def test_provider_rejects_nonfinite_nonpositive_and_unbounded_timeout(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            executable = pathlib.Path(temporary).resolve() / "provider"
            executable.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            executable.chmod(0o700)
            for timeout in (float("nan"), float("inf"), 0, -1, 301, True):
                with (
                    self.subTest(timeout=timeout),
                    self.assertRaisesRegex(RunnerError, "finite and in"),
                ):
                    ExternalAuthorityProvider(executable, timeout=timeout)

    def test_provider_blind_sandbox_denies_a_known_external_file(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary).resolve()
            denied = root / "operator-only-labels.json"
            denied.write_text('{"label":"secret"}', encoding="utf-8")
            executable = root / "provider"
            executable.write_text(
                "#!/usr/bin/env python3\n"
                "import json, pathlib, sys\n"
                f"denied = pathlib.Path({str(denied)!r})\n"
                "json.load(sys.stdin)\n"
                "try:\n"
                "    denied.read_bytes()\n"
                "except PermissionError:\n"
                "    print(json.dumps({'external_file_denied': True}))\n"
                "else:\n"
                "    print(json.dumps({'external_file_denied': False}))\n",
                encoding="utf-8",
            )
            executable.chmod(0o700)
            provider = ExternalAuthorityProvider(executable, timeout=3)
            if not provider.blind_boundary_proven:
                self.skipTest("no supported filesystem sandbox on this platform")
            response = provider.preflight({"schema": "test"})
        self.assertEqual(response, {"external_file_denied": True})


if __name__ == "__main__":
    unittest.main()
