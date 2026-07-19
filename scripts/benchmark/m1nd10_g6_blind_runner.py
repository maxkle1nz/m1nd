#!/usr/bin/env python3
"""Blind, unscored M1ND-10 G6 retrieval runner.

Every manifest repository is served by its own owner process, port, runtime
directory, registry directory, and long-lived MCP session.  Before retrieval,
that exact session performs the governed graph-ingest sequence:

    graph_ingest_preview -> authority_authorize -> external_mutation_service

The runner consumes only the public query artifact.  It has no label/scorer
input and never invokes the legacy generic ``ingest`` tool.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import pathlib
import re
import shutil
import signal
import stat
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from collections import Counter
from dataclasses import dataclass, field as dataclass_field, replace
from datetime import datetime, timezone
from typing import Any, Protocol, TextIO


PUBLIC_SCHEMA = "m1nd10-g6-public-query-corpus-v2"
HISTORICAL_PUBLIC_SCHEMA = "m1nd10-g6-public-query-corpus-v1"
SOURCE_MANIFEST_SCHEMA = "m1nd10-g6-source-manifest-v2"
CASE_SCHEMA = "m1nd10-g6-held-out-corpus-v2"
PUBLIC_TASK_FIELDS = frozenset(
    {"task_id", "repo_id", "repo_revision", "language", "repo_size_band", "query"}
)
RESULT_SCHEMA = "m1nd10-g6-retrieval-results-v2"
RUN_METADATA_SCHEMA = "m1nd10-g6-blind-run-metadata-v2"
METRIC_SPEC_SCHEMA = "m1nd10-g6-metric-spec-v2"
CALIBRATION_RUN_SCHEMA = "m1nd10-g6-calibration-run-v1"
SEEK_CALIBRATION_RECEIPT_SCHEMA = "m1nd-seek-calibration-receipt-v1"
SEEK_CALIBRATION_RECEIPT_STATUS = "calibrated"
SEEK_CALIBRATION_SIGNAL = "envelope"
SEEK_CALIBRATION_DIGEST_DOMAIN = "m1nd-seek-calibration-receipt-digest-v1"
V2_SOURCE_COMMIT = "b59a1c2a1454a83164dfb4d5640c6b005154d1ee"
VALID_VERDICTS = {"act", "reverify", "abstain"}

RESULT_FIELDS = frozenset(
    {
        "schema",
        "lane",
        "run_id",
        "corpus_id",
        "corpus_digest",
        "public_corpus_self_digest",
        "sealed_corpus_self_digest",
        "source_manifest_digest",
        "source_revision",
        "system_revision",
        "binary_digest",
        "runner_digest",
        "metric_spec_digest",
        "measurements",
        "run_metadata",
        "self_digest",
    }
)
MEASUREMENT_FIELDS = frozenset(
    {
        "task_id",
        "ranked_anchor_ids",
        "verdict",
        "north_latency_ms",
        "seek_latency_ms",
        "north_executed",
        "seek_executed",
        "ranked_scores",
        "relevance_clearing_total",
        "sufficiency",
        "trust_envelope",
    }
)
MEASUREMENT_SUFFICIENCY_FIELDS = frozenset(
    {"state", "captured", "marginal_score", "top_score"}
)
MEASUREMENT_TRUST_FIELDS = frozenset(
    {"calibrated", "score", "verdict", "calibration_receipt_digest"}
)
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

GRAPH_PREVIEW_REQUEST_SCHEMA = "m1nd-graph-ingest-preview-request-v1"
GRAPH_PREVIEW_RESPONSE_SCHEMA = "m1nd-graph-ingest-preview-response-v1"
AUTHORITY_AUTHORIZE_REQUEST_SCHEMA = "m1nd-authority-authorize-request-v1"
AUTHORITY_AUTHORIZE_RESPONSE_SCHEMA = "m1nd-authority-authorize-response-v1"
AUTHORIZATION_RECEIPT_SCHEMA = "m1nd-runtime-authorization-receipt-v1"
AUTHORIZATION_RECEIPT_DIGEST_DOMAIN = "m1nd-runtime-authorization-receipt-v1"
AUTHORIZATION_RECEIPT_SIGNATURE_DOMAIN = (
    "m1nd-runtime-authorization-receipt-signature-v1"
)
AUTHORIZATION_RECEIPT_SIGNATURE_MESSAGE_PREFIX = (
    b"m1nd-runtime-authorization-receipt-signature-message-v1\0"
)
EXTERNAL_MUTATION_REQUEST_SCHEMA = "m1nd-external-mutation-request-v1"
EXTERNAL_MUTATION_RESPONSE_SCHEMA = "m1nd-external-mutation-response-v1"
CHECKPOINT_ACK_SCHEMA = "m1nd-checkpoint-ack-v1"
CODE_OWNERSHIP_MANIFEST_SCHEMA = "m1nd-code-ownership-manifest-v1"
CODE_PIPELINE_RECEIPT_SCHEMA = "m1nd-code-pipeline-receipt-v1"
RUNNER_CHECKPOINT_SCHEMA = "m1nd10-g6-blind-runner-checkpoint-v2"

AUTHORITY_PREFLIGHT_REQUEST_SCHEMA = "m1nd10-g6-authority-preflight-request-v1"
AUTHORITY_PREFLIGHT_RESPONSE_SCHEMA = "m1nd10-g6-authority-preflight-response-v1"
AUTHORITY_ASSEMBLY_SCHEMA = "m1nd10-g6-authority-assembly-v1"
AUTHORITY_ASSEMBLY_DIGEST_DOMAIN = "m1nd10-g6-authority-assembly-v1"
VERIFICATION_KEY_REGISTRY_SCHEMA = "m1nd-verification-key-registry-v1"
AUTHORITY_PROVIDER_REQUEST_SCHEMA = "m1nd10-g6-authority-provider-request-v1"
AUTHORITY_PROVIDER_RESPONSE_SCHEMA = "m1nd10-g6-authority-provider-response-v1"
AUTHORIZATION_VERIFIER_REQUEST_SCHEMA = (
    "m1nd-g6-authorization-receipt-verification-request-v1"
)
AUTHORIZATION_VERIFIER_RESPONSE_SCHEMA = (
    "m1nd-g6-authorization-receipt-verification-proof-v1"
)
MANIFEST_DIGEST_DOMAIN = "m1nd-organism-manifest-v1"
GRAPH_INGEST_OUTCOME_DIGEST_DOMAIN = "m1nd-graph-ingest-a2-outcome-v1"
CODE_OWNERSHIP_DIGEST_DOMAIN = "m1nd-code-ownership-v1"
CODE_LINEAGE_DIGEST_DOMAIN = "m1nd-code-ingest-lineage-v1"
CODE_RESOLUTION_INPUT_DIGEST_DOMAIN = "m1nd-code-resolution-inputs-v1"
CODE_RESOLUTION_HINT_DIGEST_DOMAIN = "m1nd-code-resolution-hints-v1"
CODE_RESOLUTION_DIGEST_DOMAIN = "m1nd-code-resolution-decisions-v1"
CODE_PIPELINE_DIGEST_DOMAIN = "m1nd-code-pipeline-receipt-v1"
PROOF_NOT_PROVEN = "NOT_PROVEN"
MAX_MCP_RESPONSE_BYTES = 16 * 1024 * 1024
MAX_PROVIDER_INPUT_BYTES = 256 * 1024
MAX_PROVIDER_STDOUT_BYTES = 1024 * 1024
MAX_PROVIDER_STDERR_BYTES = 64 * 1024
MAX_PROVIDER_TIMEOUT_SECONDS = 300.0
MAX_PRIVATE_JSON_BYTES = 64 * 1024
MAX_TOKEN_BYTES = 65
AUTHORITY_CLOCK_SKEW_MS = 5 * 60 * 1000
MAX_AUTHORITY_CLOCK_SKEW_MS = 30_000
MAX_AUTHORIZATION_LEASE_MS = 5 * 60 * 1000
INSTALLED_OWNER_PORT = 1338

AUTHORIZATION_RECEIPT_FIELDS = frozenset(
    {"schema", "core", "receipt_digest", "issuer", "key_id", "algorithm", "signature"}
)
AUTHORIZATION_RECEIPT_CORE_FIELDS = frozenset(
    {
        "organism_id",
        "repo_id",
        "brain_id",
        "subject_id",
        "role",
        "capability_id",
        "capability_kind",
        "verified_object_digest",
        "mission_id",
        "mission_head_id",
        "transport_session_id",
        "ingress_context_digest",
        "action",
        "ingress",
        "complete_effects",
        "active_mode",
        "constitution_digest",
        "constitution_epoch",
        "autonomy_epoch",
        "protected_epoch_at_decision",
        "policy_registry_digest",
        "exact_policy_tuple",
        "authority_decision_digest",
        "autonomy_admission_receipt_digest",
        "autonomy_committed_state_digest",
        "autonomy_protected_root_digest",
        "authority",
        "authority_body_digest",
        "replay_sequence",
        "journal_sequence",
        "journal_root_digest",
        "protected_epoch",
        "authorized_at",
        "expires_at",
    }
)
AUTHORITY_VERIFICATION_KEY_FIELDS = frozenset(
    {
        "key_id",
        "subject_id",
        "algorithm",
        "public_key",
        "created_at",
        "activated_at",
        "expires_at",
        "revoked_at",
        "rotated_at",
        "replacement_key_id",
        "status",
    }
)
AUTHORITY_ASSEMBLY_FIELDS = frozenset(
    {
        "schema",
        "assembly_id",
        "provider_kind",
        "production_authority_assembly",
        "owner_binary_digest",
        "provider_executable_digest",
        "owner_security_config_digest",
        "verification_key_registry",
        "receipt_key_id",
        "max_future_clock_skew_ms",
        "self_digest",
    }
)
VERIFICATION_KEY_REGISTRY_FIELDS = frozenset({"schema", "registry_epoch", "keys"})
EXACT_POLICY_TUPLE_FIELDS = frozenset(
    {
        "ingress",
        "action",
        "active_mode",
        "subject_id",
        "authority_variant",
        "applicable_grant_id",
        "applicable_tier",
        "risk_class",
    }
)
SOURCE_CLAIMS_FIELDS = frozenset({"source_hint", "node_ids", "edges"})
CLAIMED_EDGE_FIELDS = frozenset(
    {"source", "target", "relation", "direction", "inhibitory"}
)
RESOLUTION_INPUT_FIELDS = frozenset(
    {"source_key", "source_id", "target_label", "relation"}
)
RESOLUTION_HINT_FIELDS = frozenset({"source_id", "target_label", "import_path"})
RESOLUTION_DECISION_FIELDS = frozenset(
    {
        "source_key",
        "source_id",
        "target_label",
        "relation",
        "outcome",
        "resolved_target_id",
        "candidate_ids",
        "source_line_start",
        "source_line_end",
    }
)
GRAPH_INGEST_RESULT_FIELDS = frozenset(
    {
        "mode",
        "root_identity",
        "reconciliation_brain_id",
        "ownership_manifest",
        "parent",
        "candidate_ownership_digest",
        "candidate_source_projection_digest",
        "candidate_pipeline_digest",
        "actor_checkpoint_required",
        "ingest_output",
        "graph_generation_before",
        "graph_generation_after",
        "checkpoint_ack",
    }
)
CHECKPOINT_ACK_FIELDS = frozenset(
    {
        "schema",
        "checkpoint_id",
        "brain_id",
        "epoch",
        "generation",
        "revision",
        "current_pointer_digest",
        "confirmed_at_unix_ms",
    }
)
OWNERSHIP_MANIFEST_FIELDS = frozenset(
    {
        "schema",
        "root_identity",
        "exact_source_key",
        "base_ownership_digest",
        "source_digests",
        "claims_by_source",
        "source_projection_digest",
        "graph_finalized",
        "pending_edge_count",
        "bidirectional_mirrors_valid",
        "csr_shape_valid",
        "reverse_csr_valid",
        "orphan_node_slots",
        "multiply_identified_node_slots",
        "invalid_identity_ids",
        "out_of_range_identity_ids",
        "orphan_edge_slots",
        "resolution_inputs",
        "resolution_input_digest",
        "resolution_hints",
        "resolution_hint_digest",
        "resolution_decisions",
        "resolution_digest",
        "pipeline_receipt",
        "pipeline_digest",
        "coverage",
        "unowned_nodes",
        "unowned_edges",
        "dangling_node_claims",
        "dangling_edge_claims",
        "duplicate_graph_edges",
        "lineage_digest",
        "ownership_digest",
    }
)
PIPELINE_RECEIPT_FIELDS = frozenset(
    {
        "schema",
        "pipeline_version",
        "producer_name",
        "producer_version",
        "producer_build_identity",
        "producer_executable_identity",
        "skip_dirs",
        "skip_files",
        "include_dotfiles",
        "dotfile_patterns",
        "policy_fingerprint",
        "build_features",
        "binary_policy",
        "vcs_context_digest",
        "immutable_source_snapshot",
        "discovered_source_count",
        "extracted_source_count",
        "digested_source_count",
        "global_enrichment_enabled",
        "cross_file_source_files_expected",
        "cross_file_source_metadata_verified",
        "cross_file_source_files_read",
        "cross_file_source_files_parsed",
        "cargo_workspace_members_expected",
        "cargo_workspace_members_accounted",
        "cargo_dependency_inputs_expected",
        "cargo_dependency_inputs_accounted",
        "cargo_package_file_links_expected",
        "cargo_package_file_links_accounted",
    }
)
PIPELINE_RECEIPT_ORDER = (
    "schema",
    "pipeline_version",
    "producer_name",
    "producer_version",
    "producer_build_identity",
    "producer_executable_identity",
    "skip_dirs",
    "skip_files",
    "include_dotfiles",
    "dotfile_patterns",
    "policy_fingerprint",
    "build_features",
    "binary_policy",
    "vcs_context_digest",
    "immutable_source_snapshot",
    "discovered_source_count",
    "extracted_source_count",
    "digested_source_count",
    "global_enrichment_enabled",
    "cross_file_source_files_expected",
    "cross_file_source_metadata_verified",
    "cross_file_source_files_read",
    "cross_file_source_files_parsed",
    "cargo_workspace_members_expected",
    "cargo_workspace_members_accounted",
    "cargo_dependency_inputs_expected",
    "cargo_dependency_inputs_accounted",
    "cargo_package_file_links_expected",
    "cargo_package_file_links_accounted",
)
INGEST_OUTPUT_FIELDS = frozenset(
    {
        "mode",
        "adapter",
        "namespace",
        "files_scanned",
        "files_parsed",
        "nodes_created",
        "edges_created",
        "elapsed_ms",
        "node_count",
        "edge_count",
        "light_evidence_resolved",
        "light_evidence_unresolved",
        "memory_freshness",
    }
)
RUNNER_CHECKPOINT_FIELDS = frozenset(
    {
        "schema",
        "lane",
        "run_id",
        "corpus_id",
        "corpus_digest",
        "public_corpus_self_digest",
        "source_manifest_digest",
        "source_revision",
        "system_revision",
        "binary_digest",
        "runner_digest",
        "metric_spec_digest",
        "sealed_corpus_self_digest",
        "owner_ingest_bindings",
        "completed",
        "task_count",
        "measurement_task_ids",
        "measurements",
        "resume_supported",
        "generated_at",
        "self_digest",
    }
)
CHECKPOINT_OWNER_BINDING_FIELDS = frozenset(
    {
        "repo_id",
        "owner_id",
        "source_revision",
        "file_set_digest",
        "mcp_session_id",
        "candidate_ownership_digest",
        "candidate_source_projection_digest",
        "candidate_pipeline_digest",
        "checkpoint_id",
        "checkpoint_generation",
        "current_pointer_digest",
    }
)

GRAPH_INGEST_EFFECTS = frozenset(
    {"GRAPH_MUTATION", "RUNTIME_STORE_WRITE", "SOVEREIGN_MUTATION"}
)
CROSS_FILE_SOURCE_EXTENSIONS = frozenset(
    {
        "py",
        "ts",
        "tsx",
        "js",
        "jsx",
        "mjs",
        "cjs",
        "go",
        "java",
        "rs",
        "h",
        "hpp",
        "hxx",
        "hh",
        "c",
        "cc",
        "cpp",
        "cxx",
        "kt",
        "kts",
        "php",
        "scala",
        "sc",
        "rb",
    }
)
PREVIEW_FIELDS = frozenset(
    {
        "schema",
        "request_id",
        "preview_id",
        "semantic_action",
        "requested_effects",
        "authority_floor",
        "risk_class",
        "ingress",
        "route_selector",
        "actor_brain_id",
        "transport_session_id",
        "ingress_context_digest",
        "root_identity",
        "expected_graph_generation",
        "expected_source_projection_digest",
        "candidate_ownership_digest",
        "candidate_source_projection_digest",
        "candidate_pipeline_digest",
        "scan_job_id",
        "semantic_payload_digest",
        "operation_object_digest",
        "authority_binding",
        "execute_request",
    }
)
AUTHORIZATION_REQUEST_FIELDS = frozenset(
    {
        "schema",
        "request_id",
        "authority_session_id",
        "authority_session_context_digest",
        "target_action",
        "payload_digest",
        "requested_effects",
        "mission_id",
        "mission_head_id",
        "input",
    }
)


class RunnerError(RuntimeError):
    """A fail-closed runner error."""


@dataclass(frozen=True)
class OwnerSpec:
    """One repository's fully isolated served-owner topology."""

    repo_id: str
    source_revision: str
    file_set_digest: str
    root: pathlib.Path
    runtime_dir: pathlib.Path
    registry_dir: pathlib.Path
    port: int
    owner_id: str
    scope: str
    source_digests: tuple[tuple[str, str], ...] = ()

    @property
    def base_url(self) -> str:
        return f"http://127.0.0.1:{self.port}"

    @property
    def token_file(self) -> pathlib.Path:
        return self.runtime_dir / "http-auth-token-v1"

    @property
    def log_path(self) -> pathlib.Path:
        return self.runtime_dir / "owner.log"


@dataclass(frozen=True)
class AuthorityVerificationKey:
    """Exact public-key record pinned by the authority assembly."""

    key_id: str
    subject_id: str
    algorithm: str
    public_key: str
    created_at: int
    activated_at: int
    expires_at: int | None
    revoked_at: int | None
    rotated_at: int | None
    replacement_key_id: str | None
    status: str

    def as_wire(self) -> dict[str, Any]:
        return {
            "key_id": self.key_id,
            "subject_id": self.subject_id,
            "algorithm": self.algorithm,
            "public_key": self.public_key,
            "created_at": self.created_at,
            "activated_at": self.activated_at,
            "expires_at": self.expires_at,
            "revoked_at": self.revoked_at,
            "rotated_at": self.rotated_at,
            "replacement_key_id": self.replacement_key_id,
            "status": self.status,
        }


@dataclass(frozen=True)
class AuthorityAssembly:
    """Public, non-secret identity of the authority provider assembly."""

    provider_kind: str
    production_authority_assembly: bool
    assembly_id: str
    assembly_digest: str
    binary_digest: str
    provider_executable_digest: str
    owner_security_config_digest: str
    key_registry_epoch: int
    max_future_clock_skew_ms: int
    verification_key: AuthorityVerificationKey
    expected_digest_verified: bool
    blind_boundary_kind: str
    blind_boundary_proven: bool


@dataclass(frozen=True)
class CapturedBearerToken:
    """A single safely opened bearer; its value is never rendered or reread."""

    value: str = dataclass_field(repr=False)
    path: pathlib.Path
    file_identity: tuple[int, int, int, int, int, int, int, int]
    parent_identity: tuple[int, int]

    def assert_unchanged(self) -> None:
        try:
            parent = self.path.parent.lstat()
            current = self.path.lstat()
        except OSError as error:
            raise RunnerError("owner bearer path changed after capture") from error
        if stat.S_ISLNK(current.st_mode) or not stat.S_ISREG(current.st_mode):
            raise RunnerError("owner bearer path changed after capture")
        if (parent.st_dev, parent.st_ino) != self.parent_identity:
            raise RunnerError("owner bearer parent changed after capture")
        if (
            not stat.S_ISDIR(parent.st_mode)
            or parent.st_uid != os.geteuid()
            or stat.S_IMODE(parent.st_mode) & 0o077
        ):
            raise RunnerError("owner bearer parent privacy changed after capture")
        identity = (
            current.st_dev,
            current.st_ino,
            current.st_size,
            current.st_uid,
            stat.S_IMODE(current.st_mode),
            current.st_mtime_ns,
            current.st_ctime_ns,
            current.st_nlink,
        )
        if identity != self.file_identity:
            raise RunnerError("owner bearer identity changed after capture")


@dataclass(frozen=True)
class OwnerReadinessReceipt:
    """Proof that the contacted endpoint is the exact spawned owner."""

    instance_id: str
    pid: int
    started_at_ms: int
    port: int
    registry_entry_digest: str
    manifest_digest: str
    binary_digest: str
    token_captured_once: bool
    owner_binding_proven: bool


@dataclass
class OwnerHandle:
    spec: OwnerSpec
    process: subprocess.Popen[Any]
    log: TextIO
    client: "McpHttpClient"
    initial_session_id: str
    readiness: OwnerReadinessReceipt


class AuthorityProvider(Protocol):
    @property
    def identity_digest(self) -> str: ...

    @property
    def blind_boundary_kind(self) -> str: ...

    @property
    def blind_boundary_proven(self) -> bool: ...

    def preflight(self, request: dict[str, Any]) -> dict[str, Any]: ...

    def authorize(self, request: dict[str, Any]) -> dict[str, Any]: ...


class AuthorizationReceiptVerifier(Protocol):
    def verify(
        self,
        receipt: dict[str, Any],
        key: AuthorityVerificationKey,
        max_future_clock_skew_ms: int,
    ) -> dict[str, Any]: ...


def _canonical_bytes(value: Any) -> bytes:
    return json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")


def _sha256_bytes(value: bytes) -> str:
    return "sha256:" + hashlib.sha256(value).hexdigest()


def _without_key(value: dict[str, Any], key: str) -> dict[str, Any]:
    return {name: item for name, item in value.items() if name != key}


def _self_digest(value: dict[str, Any]) -> str:
    return _sha256_bytes(_canonical_bytes(_without_key(value, "self_digest")))


def _source_line_count(content: bytes) -> int:
    return content.count(b"\n") + int(bool(content) and not content.endswith(b"\n"))


def _size_band(lines: int) -> str:
    if lines < 10_000:
        return "small"
    if lines < 100_000:
        return "medium"
    return "large"


def _validate_source_manifest_contract(manifest: Any) -> list[dict[str, Any]]:
    manifest = _require_exact_fields(
        manifest, SOURCE_MANIFEST_FIELDS, "public source manifest"
    )
    if (
        manifest["schema"] != SOURCE_MANIFEST_SCHEMA
        or manifest["snapshot_kind"] != "immutable_git_objects"
        or manifest["worktree_state_excluded"] is not True
    ):
        raise RunnerError(
            "public source manifest is not the held-out-v2 immutable contract"
        )
    source_commit = manifest["source_commit"]
    if source_commit != V2_SOURCE_COMMIT:
        raise RunnerError(
            "public source manifest commit is not the held-out-v2 snapshot"
        )
    if manifest["manifest_digest"] != _sha256_bytes(
        _canonical_bytes(_without_key(manifest, "manifest_digest"))
    ):
        raise RunnerError("public source manifest digest mismatch")
    repos = manifest["repos"]
    if not isinstance(repos, list) or len(repos) != 4:
        raise RunnerError("held-out-v2 source manifest must contain exactly four repos")
    repo_ids: set[str] = set()
    roots: set[str] = set()
    for repo in repos:
        repo = _require_exact_fields(repo, REPO_MANIFEST_FIELDS, "source repository")
        repo_id = _require_nonempty_string(repo["repo_id"], "repo_id")
        source_root = _require_nonempty_string(repo["source_root"], "source_root")
        if repo_id in repo_ids or source_root in roots:
            raise RunnerError("source manifest repo ids/roots must be unique")
        repo_ids.add(repo_id)
        roots.add(source_root)
        _canonical_relative_posix(source_root, "source manifest root")
        tree = repo["git_tree"]
        if not isinstance(tree, str) or re.fullmatch(r"[0-9a-f]{40}", tree) is None:
            raise RunnerError(f"{repo_id} git tree is malformed")
        if repo["source_revision"] != f"git:{source_commit}:tree:{tree}":
            raise RunnerError(f"{repo_id} source revision is not pinned to commit/tree")
        if (
            repo["primary_language"] not in {"rust", "python", "typescript"}
            or repo["repo_size_band"] not in SIZE_BAND_DEFINITION
            or repo["size_band_definition"] != SIZE_BAND_DEFINITION
        ):
            raise RunnerError(f"{repo_id} language/size-band contract is malformed")
        files = repo["files"]
        if not isinstance(files, list) or not files:
            raise RunnerError(f"{repo_id} file manifest is absent")
        if repo["file_set_digest"] != _sha256_bytes(_canonical_bytes(files)):
            raise RunnerError(f"{repo_id} file_set_digest mismatch")
        paths: set[str] = set()
        source_files = 0
        source_lines = 0
        for entry in files:
            entry = _require_exact_fields(
                entry, FILE_MANIFEST_FIELDS, "source file entry"
            )
            relative = _require_nonempty_string(entry["path"], "source file path")
            _canonical_relative_posix(relative, f"{repo_id} source file path")
            if relative in paths:
                raise RunnerError(f"{repo_id} source file paths are duplicated")
            paths.add(relative)
            if entry["role"] not in {"source", "dependency_manifest"}:
                raise RunnerError(f"{repo_id} source file role is unsupported")
            if (
                not isinstance(entry["bytes"], int)
                or isinstance(entry["bytes"], bool)
                or entry["bytes"] < 0
                or not isinstance(entry["lines"], int)
                or isinstance(entry["lines"], bool)
                or entry["lines"] < 0
                or re.fullmatch(r"sha256:[0-9a-f]{64}", str(entry["sha256"])) is None
            ):
                raise RunnerError(f"{repo_id} source file metadata is malformed")
            if entry["role"] == "source":
                source_files += 1
                source_lines += entry["lines"]
        if (
            repo["searched_file_count"] != len(files)
            or repo["source_file_count"] != source_files
            or repo["source_line_count"] != source_lines
            or repo["repo_size_band"] != _size_band(source_lines)
        ):
            raise RunnerError(f"{repo_id} aggregate source manifest counts mismatch")
    return repos


def validate_public_queries(queries: dict[str, Any]) -> list[dict[str, Any]]:
    """Accept only the real held-out-v2 public surface and prove its digests."""
    if queries.get("schema") == HISTORICAL_PUBLIC_SCHEMA:
        raise RunnerError(
            "historical held-out-v1 public evidence is not formal G6 evidence"
        )
    _require_exact_fields(queries, PUBLIC_FIELDS, "public query corpus")
    if (
        queries["schema"] != PUBLIC_SCHEMA
        or queries["version"] != 2
        or queries["blinded"] is not True
        or queries["author_review_status"] != "AUTHOR_ONLY_AWAITING_INDEPENDENT_REVIEW"
    ):
        raise RunnerError(
            "queries artifact is not the held-out-v2 blinded public schema"
        )
    if queries["self_digest"] != _self_digest(queries):
        raise RunnerError("public query corpus self_digest mismatch")
    repos = _validate_source_manifest_contract(queries["source_manifest"])
    tasks = queries.get("tasks")
    if (
        not isinstance(tasks, list)
        or len(tasks) != 220
        or queries.get("task_count") != 220
    ):
        raise RunnerError("public task_count disagrees with tasks")
    task_ids: list[str | None] = []
    repo_map = {repo["repo_id"]: repo for repo in repos}
    for task in tasks:
        if not isinstance(task, dict) or set(task) != PUBLIC_TASK_FIELDS:
            raise RunnerError(
                "public task contains missing, unknown, or label-bearing fields"
            )
        repo = repo_map.get(task.get("repo_id"))
        if repo is None or task.get("repo_revision") != repo["source_revision"]:
            raise RunnerError("public task has a foreign repo/revision binding")
        if task.get("language") != repo["primary_language"]:
            raise RunnerError("public task language differs from its repo binding")
        if task.get("repo_size_band") != repo["repo_size_band"]:
            raise RunnerError("public task size band differs from its repo binding")
        if (
            not isinstance(task.get("task_id"), str)
            or re.fullmatch(r"g6-(?:mcp|core|py|ui)-[0-9a-f]{16}", task["task_id"])
            is None
            or not isinstance(task.get("query"), str)
            or len(task["query"]) < 55
            or not task["query"].endswith("?")
        ):
            raise RunnerError("public task query violates the held-out-v2 surface")
        task_ids.append(task.get("task_id"))
    if None in task_ids or len(task_ids) != len(set(task_ids)):
        raise RunnerError("public task IDs are missing or duplicated")
    expected_corpus_digest = _sha256_bytes(
        _canonical_bytes(
            {
                "source_manifest_digest": queries["source_manifest"]["manifest_digest"],
                "tasks": tasks,
            }
        )
    )
    expected_corpus_id = (
        "m1nd10-g6-held-out-v2-" + expected_corpus_digest.removeprefix("sha256:")[:16]
    )
    if (
        queries["corpus_digest"] != expected_corpus_digest
        or queries["corpus_id"] != expected_corpus_id
    ):
        raise RunnerError("public corpus digest/id binding mismatch")
    runner_contract = _require_exact_fields(
        queries["runner_contract"], RUNNER_CONTRACT_FIELDS, "public runner contract"
    )
    if (
        runner_contract["labels_exposed"] is not False
        or runner_contract["forbidden_artifact"] != "operator-only/corpus.json"
        or runner_contract["read_only_artifact"] != "public/queries.json"
        or runner_contract["independent_review_status"] != "NOT_RUN"
        or runner_contract["result_coverage"]
        != "emit exactly one measurement for every task_id"
    ):
        raise RunnerError("public runner contract is absent or exposes labels")
    if (
        runner_contract.get("source_checkout")
        != queries["source_manifest"]["source_commit"]
    ):
        raise RunnerError("public runner contract source checkout mismatch")
    return tasks


def public_source_revision(queries: dict[str, Any]) -> str:
    """Return the immutable global source identity before runtime work begins."""
    manifest = queries.get("source_manifest")
    if not isinstance(manifest, dict):
        raise RunnerError("public source manifest is absent")
    revision = manifest.get("source_commit") or manifest.get("snapshot_digest")
    if not isinstance(revision, str) or not revision.strip():
        raise RunnerError("public source manifest lacks an immutable source identity")
    return revision


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def _sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def _write_json_durable(path: pathlib.Path, value: dict[str, Any]) -> None:
    """Atomically replace ``path`` and fsync both bytes and directory entry."""
    payload = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")
    temporary = path.with_name(
        f".{path.name}.{os.getpid()}.{threading.get_ident()}.{time.time_ns()}.tmp"
    )
    descriptor: int | None = None
    try:
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "wb") as handle:
            descriptor = None
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if descriptor is not None:
            os.close(descriptor)
        try:
            temporary.unlink()
        except FileNotFoundError:
            pass


def _write_json_atomic(path: pathlib.Path, value: dict[str, Any]) -> None:
    """Compatibility name; all runner JSON writes are now durable."""
    _write_json_durable(path, value)


def _canonical_relative_posix(value: Any, label: str) -> pathlib.PurePosixPath:
    relative = _require_nonempty_string(value, label)
    if "\\" in relative or "\x00" in relative:
        raise RunnerError(f"{label} is not a canonical POSIX relative path")
    pure = pathlib.PurePosixPath(relative)
    if (
        pure.is_absolute()
        or pure.as_posix() != relative
        or any(part in {"", ".", ".."} for part in pure.parts)
    ):
        raise RunnerError(f"{label} escapes or is non-canonical")
    return pure


def _assert_absolute(path: pathlib.Path, label: str) -> pathlib.Path:
    if not path.is_absolute():
        raise RunnerError(f"{label} must be absolute")
    return path


def _assert_no_symlink_components(path: pathlib.Path, label: str) -> None:
    """Reject symlinks, including a broken symlink at a missing target."""
    _assert_absolute(path, label)
    current = pathlib.Path(path.anchor)
    for part in path.parts[1:]:
        current /= part
        if os.path.lexists(current):
            if current.is_symlink():
                raise RunnerError(f"{label} contains a symlink component")
        else:
            break


def _normalized_absolute(path: pathlib.Path) -> pathlib.Path:
    return pathlib.Path(os.path.abspath(os.path.normpath(path)))


def _paths_overlap(left: pathlib.Path, right: pathlib.Path) -> bool:
    left = _normalized_absolute(left)
    right = _normalized_absolute(right)
    try:
        left.relative_to(right)
        return True
    except ValueError:
        pass
    try:
        right.relative_to(left)
        return True
    except ValueError:
        return False


def validate_runner_paths(args: argparse.Namespace) -> dict[str, Any]:
    """Fail before reads/spawns unless every mutable root is fresh and isolated."""
    file_inputs = {
        "queries": args.queries,
        "metric_spec": args.metric_spec,
        "binary": args.binary,
    }
    if args.authority_provider is not None:
        file_inputs["authority_provider"] = args.authority_provider
    if getattr(args, "authority_assembly", None) is not None:
        file_inputs["authority_assembly"] = args.authority_assembly
    for label, path in file_inputs.items():
        _assert_absolute(path, label)
        _assert_no_symlink_components(path, label)
        if not path.is_file():
            raise RunnerError(f"{label} must be an existing regular file")

    source = _assert_absolute(args.source_root, "source_root")
    _assert_no_symlink_components(source, "source_root")
    if not source.is_dir():
        raise RunnerError("source_root must be an existing directory")

    mutable_roots = {
        "runtime_dir": _assert_absolute(args.runtime_dir, "runtime_dir"),
        "registry_dir": _assert_absolute(args.registry_dir, "registry_dir"),
    }
    for label, path in mutable_roots.items():
        _assert_no_symlink_components(path, label)
        if os.path.lexists(path):
            raise RunnerError(f"{label} must be fresh and absent")
        if not path.parent.is_dir():
            raise RunnerError(f"{label} parent must already exist")

    output_paths = {"output": _assert_absolute(args.output, "output")}
    if args.checkpoint is not None:
        output_paths["checkpoint"] = _assert_absolute(args.checkpoint, "checkpoint")
    for label, path in output_paths.items():
        _assert_no_symlink_components(path, label)
        if os.path.lexists(path):
            raise RunnerError(f"{label} must be fresh and absent")
        if not path.parent.is_dir():
            raise RunnerError(f"{label} parent must already exist")

    topology = {"source_root": source, **mutable_roots, **output_paths}
    names = list(topology)
    for index, left_name in enumerate(names):
        for right_name in names[index + 1 :]:
            left = topology[left_name]
            right = topology[right_name]
            if _paths_overlap(left, right):
                raise RunnerError(
                    f"{left_name} and {right_name} overlap; roots must be disjoint"
                )
    return {
        "absolute": True,
        "fresh_mutable_roots": True,
        "disjoint": True,
        "symlink_free_path_components": True,
        "paths": {name: str(path) for name, path in topology.items()},
    }


def _safe_repo_component(repo_id: str) -> str:
    stem = re.sub(r"[^A-Za-z0-9_.-]+", "-", repo_id).strip("-.") or "repo"
    suffix = hashlib.sha256(repo_id.encode("utf-8")).hexdigest()[:10]
    return f"{stem[:48]}-{suffix}"


def _require_nonempty_string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise RunnerError(f"{label} must be a non-empty string")
    return value


def _require_digest(value: Any, label: str, *, prefixed: bool | None = None) -> str:
    value = _require_nonempty_string(value, label)
    if prefixed is True:
        valid = bool(re.fullmatch(r"sha256:[0-9a-f]{64}", value))
    elif prefixed is False:
        valid = bool(re.fullmatch(r"[0-9a-f]{64}", value))
    else:
        valid = bool(re.fullmatch(r"(?:sha256:)?[0-9a-f]{64}", value))
    if not valid:
        raise RunnerError(f"{label} is not a canonical SHA-256 digest")
    return value


def _require_exact_fields(
    value: Any, fields: frozenset[str], label: str
) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != fields:
        raise RunnerError(f"{label} violates its closed JSON field set")
    return value


def _require_u64(value: Any, label: str) -> int:
    if (
        not isinstance(value, int)
        or isinstance(value, bool)
        or value < 0
        or value > (1 << 64) - 1
    ):
        raise RunnerError(f"{label} must be a canonical unsigned 64-bit integer")
    return value


def _authority_assembly_digest(value: dict[str, Any]) -> str:
    return _rust_domain_digest(
        AUTHORITY_ASSEMBLY_DIGEST_DOMAIN, _without_key(value, "self_digest")
    )


def _parse_active_verification_key(
    value: Any,
    *,
    expected_key_id: str,
    now_ms: int,
    max_future_clock_skew_ms: int,
    production: bool,
) -> AuthorityVerificationKey:
    key = _require_exact_fields(
        value, AUTHORITY_VERIFICATION_KEY_FIELDS, "authority verification key"
    )
    key_id = _require_nonempty_string(key["key_id"], "verification key_id")
    subject_id = _require_nonempty_string(
        key["subject_id"], "verification key subject_id"
    )
    algorithm = _require_nonempty_string(key["algorithm"], "verification key algorithm")
    public_key = _require_nonempty_string(
        key["public_key"], "verification key public_key"
    )
    if key_id != expected_key_id:
        raise RunnerError("authority receipt key differs from the pinned registry key")
    created_at = _require_u64(key["created_at"], "verification key created_at")
    activated_at = _require_u64(key["activated_at"], "verification key activated_at")
    expires_at = key["expires_at"]
    if expires_at is not None:
        expires_at = _require_u64(expires_at, "verification key expires_at")
    revoked_at = key["revoked_at"]
    if revoked_at is not None:
        revoked_at = _require_u64(revoked_at, "verification key revoked_at")
    rotated_at = key["rotated_at"]
    if rotated_at is not None:
        rotated_at = _require_u64(rotated_at, "verification key rotated_at")
    replacement_key_id = key["replacement_key_id"]
    if replacement_key_id is not None:
        replacement_key_id = _require_nonempty_string(
            replacement_key_id, "verification key replacement_key_id"
        )
    status = _require_nonempty_string(key["status"], "verification key status")
    if status not in {"ACTIVE", "REVOKED", "ROTATED", "EXPIRED"}:
        raise RunnerError("verification key status is invalid")
    if activated_at < created_at:
        raise RunnerError("verification key activation precedes creation")
    latest_allowed = now_ms + max_future_clock_skew_ms
    if created_at > latest_allowed or activated_at > latest_allowed:
        raise RunnerError("verification key is not yet active")
    if status != "ACTIVE":
        raise RunnerError("authority receipt key is not ACTIVE")
    if (
        revoked_at is not None
        or rotated_at is not None
        or replacement_key_id is not None
    ):
        raise RunnerError("ACTIVE verification key carries terminal lifecycle fields")
    if expires_at is not None and (expires_at <= activated_at or now_ms >= expires_at):
        raise RunnerError("authority receipt key is expired")
    if production and (
        algorithm != "ED25519" or re.fullmatch(r"[0-9a-f]{64}", public_key) is None
    ):
        raise RunnerError("production assembly lacks a canonical Ed25519 public key")
    return AuthorityVerificationKey(
        key_id=key_id,
        subject_id=subject_id,
        algorithm=algorithm,
        public_key=public_key,
        created_at=created_at,
        activated_at=activated_at,
        expires_at=expires_at,
        revoked_at=revoked_at,
        rotated_at=rotated_at,
        replacement_key_id=replacement_key_id,
        status=status,
    )


def load_authority_assembly(
    value: Any,
    *,
    expected_digest: str,
    binary_digest: str,
    provider_executable_digest: str,
    blind_boundary_kind: str,
    blind_boundary_proven: bool,
    now_ms: int | None = None,
) -> AuthorityAssembly:
    """Load an independently pinned assembly; provider claims cannot replace it."""
    assembly = _require_exact_fields(
        value, AUTHORITY_ASSEMBLY_FIELDS, "authority assembly manifest"
    )
    if assembly["schema"] != AUTHORITY_ASSEMBLY_SCHEMA:
        raise RunnerError("authority assembly schema mismatch")
    expected_digest = _require_digest(
        expected_digest, "expected authority assembly digest", prefixed=False
    )
    observed_digest = _require_digest(
        assembly["self_digest"], "authority assembly self_digest", prefixed=False
    )
    recomputed = _authority_assembly_digest(assembly)
    if observed_digest != recomputed or observed_digest != expected_digest:
        raise RunnerError("authority assembly digest is not independently pinned")
    provider_kind = _require_nonempty_string(
        assembly["provider_kind"], "authority provider_kind"
    )
    if provider_kind not in {"production", "software_test"}:
        raise RunnerError("authority assembly provider kind is invalid")
    production = assembly["production_authority_assembly"]
    if not isinstance(production, bool):
        raise RunnerError("authority production assembly flag must be boolean")
    owner_binary = _require_digest(
        assembly["owner_binary_digest"], "assembly owner binary", prefixed=True
    )
    provider_binary = _require_digest(
        assembly["provider_executable_digest"],
        "assembly provider executable",
        prefixed=True,
    )
    if owner_binary != binary_digest:
        raise RunnerError("authority assembly is bound to a foreign owner binary")
    if provider_binary != provider_executable_digest:
        raise RunnerError(
            "authority assembly is bound to a foreign provider executable"
        )
    owner_security_config_digest = _require_digest(
        assembly["owner_security_config_digest"],
        "assembly owner security config",
        prefixed=True,
    )
    max_skew = _require_u64(
        assembly["max_future_clock_skew_ms"], "assembly authority clock skew"
    )
    if max_skew > MAX_AUTHORITY_CLOCK_SKEW_MS:
        raise RunnerError("authority assembly widens the maximum clock skew")
    registry = _require_exact_fields(
        assembly["verification_key_registry"],
        VERIFICATION_KEY_REGISTRY_FIELDS,
        "verification key registry",
    )
    if registry["schema"] != VERIFICATION_KEY_REGISTRY_SCHEMA:
        raise RunnerError("verification key registry schema mismatch")
    registry_epoch = _require_u64(
        registry["registry_epoch"], "verification key registry epoch"
    )
    if registry_epoch == 0:
        raise RunnerError("verification key registry epoch must be positive")
    receipt_key_id = _require_nonempty_string(
        assembly["receipt_key_id"], "assembly receipt_key_id"
    )
    keys = registry["keys"]
    if not isinstance(keys, dict) or set(keys) != {receipt_key_id}:
        raise RunnerError("authority assembly must pin exactly one receipt key")
    checked_at = int(time.time_ns() // 1_000_000) if now_ms is None else now_ms
    _require_u64(checked_at, "authority assembly check time")
    verification_key = _parse_active_verification_key(
        keys[receipt_key_id],
        expected_key_id=receipt_key_id,
        now_ms=checked_at,
        max_future_clock_skew_ms=max_skew,
        production=production,
    )
    if production and provider_kind != "production":
        raise RunnerError("production authority assembly has a non-production provider")
    if not isinstance(blind_boundary_proven, bool):
        raise RunnerError("authority blind-boundary proof flag is invalid")
    return AuthorityAssembly(
        provider_kind=provider_kind,
        production_authority_assembly=production,
        assembly_id=_require_nonempty_string(
            assembly["assembly_id"], "authority assembly_id"
        ),
        assembly_digest=observed_digest,
        binary_digest=owner_binary,
        provider_executable_digest=provider_binary,
        owner_security_config_digest=owner_security_config_digest,
        key_registry_epoch=registry_epoch,
        max_future_clock_skew_ms=max_skew,
        verification_key=verification_key,
        expected_digest_verified=True,
        blind_boundary_kind=_require_nonempty_string(
            blind_boundary_kind, "authority blind boundary kind"
        ),
        blind_boundary_proven=blind_boundary_proven,
    )


def _same_root(left: Any, right: pathlib.Path) -> bool:
    if not isinstance(left, str) or not left.strip():
        return False
    return pathlib.Path(left).resolve() == right.resolve()


def build_owner_specs(
    queries: dict[str, Any],
    source_root: pathlib.Path,
    runtime_root: pathlib.Path,
    registry_root: pathlib.Path,
    base_port: int,
) -> list[OwnerSpec]:
    """Map every manifest repository to a distinct owner/process namespace."""
    manifest = queries.get("source_manifest")
    repos = manifest.get("repos") if isinstance(manifest, dict) else None
    if not isinstance(repos, list) or not repos:
        raise RunnerError("public source manifest has no repositories")
    if not isinstance(base_port, int) or isinstance(base_port, bool):
        raise RunnerError("legacy base port must be an integer")

    source_root = source_root.resolve()
    runtime_root = runtime_root.resolve()
    registry_root = registry_root.resolve()
    specs: list[OwnerSpec] = []
    repo_ids: set[str] = set()
    roots: set[pathlib.Path] = set()
    for index, repo in enumerate(repos):
        if not isinstance(repo, dict):
            raise RunnerError("source manifest repository is not an object")
        repo_id = _require_nonempty_string(repo.get("repo_id"), "repo_id")
        if repo_id in repo_ids:
            raise RunnerError("source manifest contains duplicate repo_id")
        repo_ids.add(repo_id)
        relative_root = _require_nonempty_string(repo.get("source_root"), "source_root")
        pure_root = _canonical_relative_posix(
            relative_root, "manifest repository source_root"
        )
        root = (source_root / pathlib.Path(*pure_root.parts)).resolve()
        try:
            root.relative_to(source_root)
        except ValueError as error:
            raise RunnerError("source_root escapes the public source root") from error
        if any(_paths_overlap(root, existing) for existing in roots):
            raise RunnerError(
                "manifest repository source roots must be distinct and non-overlapping"
            )
        roots.add(root)

        source_revision = _require_nonempty_string(
            repo.get("source_revision"), f"{repo_id} source_revision"
        )
        file_set_digest = _require_nonempty_string(
            repo.get("file_set_digest"), f"{repo_id} file_set_digest"
        )
        component = _safe_repo_component(repo_id)
        owner_id = f"g6-owner-{index + 1}-{component}"
        specs.append(
            OwnerSpec(
                repo_id=repo_id,
                source_revision=source_revision,
                file_set_digest=file_set_digest,
                root=root,
                runtime_dir=runtime_root / component,
                registry_dir=registry_root / component,
                # Port zero delegates allocation to the kernel.  The effective
                # endpoint is accepted only from this child PID's private registry.
                port=0,
                owner_id=owner_id,
                scope=f"graph.ingest.replace:{root}",
                source_digests=tuple(
                    (entry["path"], entry["sha256"].removeprefix("sha256:"))
                    for entry in repo.get("files", [])
                ),
            )
        )

    task_repos = {
        task.get("repo_id")
        for task in queries.get("tasks", [])
        if isinstance(task, dict)
    }
    unknown = task_repos - repo_ids
    if unknown:
        raise RunnerError(
            "public tasks reference repositories absent from the manifest"
        )
    revisions = {spec.repo_id: spec.source_revision for spec in specs}
    for task in queries.get("tasks", []):
        if not isinstance(task, dict) or task.get("repo_id") not in revisions:
            continue
        if task.get("repo_revision") != revisions[task["repo_id"]]:
            raise RunnerError(
                "public task revision differs from its manifest source binding"
            )
    return specs


def verify_public_source_snapshot(
    queries: dict[str, Any], source_root: pathlib.Path
) -> dict[str, Any]:
    """Prove an isolated live tree with exactly the bytes each owner will walk."""
    _assert_absolute(source_root, "source_root")
    _assert_no_symlink_components(source_root, "source_root")
    for candidate in (source_root, *source_root.parents):
        if os.path.lexists(candidate / ".git"):
            raise RunnerError(
                "source_root must be an isolated snapshot outside every Git worktree"
            )
    checked = 0
    missing = 0
    mismatched = 0
    extra = 0
    total_bytes = 0
    total_lines = 0
    repo_roots: dict[str, pathlib.Path] = {}
    for repo in queries["source_manifest"]["repos"]:
        pure_root = _canonical_relative_posix(
            repo["source_root"], f"{repo['repo_id']} source_root"
        )
        repo_root = source_root / pathlib.Path(*pure_root.parts)
        _assert_no_symlink_components(repo_root, f"{repo['repo_id']} live root")
        if not repo_root.is_dir():
            raise RunnerError(f"{repo['repo_id']} live root is absent")
        repo_roots[repo["repo_id"]] = repo_root

        observed: dict[str, pathlib.Path] = {}
        for directory, directory_names, file_names in os.walk(
            repo_root, topdown=True, followlinks=False
        ):
            base = pathlib.Path(directory)
            for name in list(directory_names):
                child = base / name
                if child.is_symlink():
                    raise RunnerError(
                        f"{repo['repo_id']} isolated snapshot contains a symlink"
                    )
                if name == ".git":
                    raise RunnerError(
                        f"{repo['repo_id']} live root is a worktree, not an isolated snapshot"
                    )
            for name in file_names:
                path = base / name
                if path.is_symlink():
                    raise RunnerError(
                        f"{repo['repo_id']} isolated snapshot contains a symlink"
                    )
                if not path.is_file():
                    raise RunnerError(
                        f"{repo['repo_id']} isolated snapshot contains a non-regular file"
                    )
                relative = path.relative_to(repo_root).as_posix()
                _canonical_relative_posix(relative, f"{repo['repo_id']} live file")
                observed[relative] = path

        expected = {entry["path"]: entry for entry in repo["files"]}
        missing_paths = set(expected) - set(observed)
        extra_paths = set(observed) - set(expected)
        missing += len(missing_paths)
        extra += len(extra_paths)
        for relative, entry in expected.items():
            path = observed.get(relative)
            if path is None:
                continue
            before = path.stat()
            checked += 1
            content = path.read_bytes()
            after = path.stat()
            if (
                before.st_dev,
                before.st_ino,
                before.st_size,
                before.st_mtime_ns,
            ) != (
                after.st_dev,
                after.st_ino,
                after.st_size,
                after.st_mtime_ns,
            ):
                raise RunnerError(
                    f"{repo['repo_id']} live source changed while it was being sealed"
                )
            lines = _source_line_count(content)
            if (
                _sha256_bytes(content) != entry["sha256"]
                or len(content) != entry["bytes"]
                or lines != entry["lines"]
            ):
                mismatched += 1
            total_bytes += len(content)
            total_lines += lines
    expected_count = sum(
        len(repo["files"]) for repo in queries["source_manifest"]["repos"]
    )
    if missing or extra or mismatched or checked != expected_count:
        raise RunnerError(
            "public source manifest mismatch "
            f"(checked={checked}, expected={expected_count}, "
            f"missing={missing}, extra={extra}, content_mismatch={mismatched})"
        )
    return {
        "checked_files": checked,
        "missing_files": missing,
        "digest_mismatches": mismatched,
        "extra_files": extra,
        "checked_bytes": total_bytes,
        "checked_lines": total_lines,
        "exact_live_file_set": True,
        "symlinks_rejected": True,
        "isolated_snapshot_required": True,
        "git_objects_used_as_live_root": False,
        "repo_roots": {key: str(value) for key, value in repo_roots.items()},
    }


def capture_private_bearer(path: pathlib.Path) -> CapturedBearerToken:
    """Open the fresh owner token once through an anchored no-follow handle."""
    path = pathlib.Path(path)
    _assert_absolute(path, "owner bearer")
    _assert_no_symlink_components(path.parent, "owner bearer parent")
    if os.name == "nt" or not hasattr(os, "O_NOFOLLOW"):
        raise RunnerError("secure no-follow owner bearer capture is unavailable")
    try:
        parent_stat = path.parent.lstat()
    except OSError as error:
        raise RunnerError("owner bearer parent is unavailable") from error
    if (
        not stat.S_ISDIR(parent_stat.st_mode)
        or parent_stat.st_uid != os.geteuid()
        or stat.S_IMODE(parent_stat.st_mode) & 0o077
    ):
        raise RunnerError("owner bearer parent is not private to the current user")
    directory_flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | os.O_NOFOLLOW
    directory_fd: int | None = None
    descriptor: int | None = None
    try:
        directory_fd = os.open(path.parent, directory_flags)
        directory_opened = os.fstat(directory_fd)
        if (directory_opened.st_dev, directory_opened.st_ino) != (
            parent_stat.st_dev,
            parent_stat.st_ino,
        ):
            raise RunnerError("owner bearer parent changed while opening")
        path_stat = os.stat(path.name, dir_fd=directory_fd, follow_symlinks=False)
        if stat.S_ISLNK(path_stat.st_mode) or not stat.S_ISREG(path_stat.st_mode):
            raise RunnerError("owner bearer is not a regular no-follow file")
        descriptor = os.open(
            path.name,
            os.O_RDONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0),
            dir_fd=directory_fd,
        )
        before = os.fstat(descriptor)
        before_identity = (
            before.st_dev,
            before.st_ino,
            before.st_size,
            before.st_uid,
            stat.S_IMODE(before.st_mode),
            before.st_mtime_ns,
            before.st_ctime_ns,
            before.st_nlink,
        )
        path_identity = (
            path_stat.st_dev,
            path_stat.st_ino,
            path_stat.st_size,
            path_stat.st_uid,
            stat.S_IMODE(path_stat.st_mode),
            path_stat.st_mtime_ns,
            path_stat.st_ctime_ns,
            path_stat.st_nlink,
        )
        if before_identity != path_identity:
            raise RunnerError("owner bearer identity changed while opening")
        if (
            before.st_uid != os.geteuid()
            or stat.S_IMODE(before.st_mode) & 0o077
            or before.st_nlink != 1
            or before.st_size not in {64, 65}
        ):
            raise RunnerError("owner bearer owner/mode/link/size contract is invalid")
        chunks = bytearray()
        while len(chunks) <= MAX_TOKEN_BYTES:
            chunk = os.read(descriptor, MAX_TOKEN_BYTES + 1 - len(chunks))
            if not chunk:
                break
            chunks.extend(chunk)
        after = os.fstat(descriptor)
        after_identity = (
            after.st_dev,
            after.st_ino,
            after.st_size,
            after.st_uid,
            stat.S_IMODE(after.st_mode),
            after.st_mtime_ns,
            after.st_ctime_ns,
            after.st_nlink,
        )
        if before_identity != after_identity:
            raise RunnerError("owner bearer changed while reading")
        payload = bytes(chunks)
    except RunnerError:
        raise
    except (OSError, UnicodeError) as error:
        raise RunnerError("owner bearer could not be captured safely") from error
    finally:
        if descriptor is not None:
            os.close(descriptor)
        if directory_fd is not None:
            os.close(directory_fd)
    if re.fullmatch(rb"[0-9a-f]{64}(?:\n)?", payload) is None:
        raise RunnerError("owner bearer is not canonical lowercase hex")
    value = payload.removesuffix(b"\n").decode("ascii")
    return CapturedBearerToken(
        value=value,
        path=path,
        file_identity=before_identity,
        parent_identity=(parent_stat.st_dev, parent_stat.st_ino),
    )


class McpHttpClient:
    def __init__(
        self,
        base_url: str,
        caller_root: pathlib.Path,
        bearer_token: CapturedBearerToken,
        client_name: str,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.caller_root = str(caller_root.resolve())
        self.bearer_token = bearer_token
        self.client_name = client_name
        self.session_id: str | None = None
        self.authorization_lease_id: str | None = None
        self.request_id = 0

    def _headers(self) -> dict[str, str]:
        self.bearer_token.assert_unchanged()
        headers = {
            "Accept": "application/json, text/event-stream",
            "Content-Type": "application/json",
            "M1nd-Caller-Root": self.caller_root,
            "Authorization": f"Bearer {self.bearer_token.value}",
        }
        if self.session_id:
            headers["Mcp-Session-Id"] = self.session_id
        if self.authorization_lease_id:
            headers["M1nd-Authority-Lease-Id"] = self.authorization_lease_id
        return headers

    def bind_authorization_lease(self, lease_id: str) -> None:
        _require_digest(lease_id, "authorization lease", prefixed=False)
        if self.authorization_lease_id is not None:
            raise RunnerError(
                "an authorization lease is already bound to this MCP session"
            )
        self.authorization_lease_id = lease_id

    def clear_authorization_lease(self) -> None:
        self.authorization_lease_id = None

    def get_json(
        self, path: str, *, timeout: float = 30.0
    ) -> tuple[dict[str, Any], bytes]:
        if not path.startswith("/") or "?" in path or "#" in path:
            raise RunnerError("owner attestation path is not closed")
        request = urllib.request.Request(
            f"{self.base_url}{path}",
            headers={
                "Accept": "application/json",
                "Cache-Control": "no-store",
                **self._headers(),
            },
            method="GET",
        )
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                status = getattr(response, "status", response.getcode())
                body = response.read(MAX_MCP_RESPONSE_BYTES + 1)
        except urllib.error.HTTPError as error:
            error.read(2001)
            raise RunnerError(
                f"owner attestation endpoint returned HTTP {error.code}"
            ) from error
        except OSError as error:
            raise RunnerError("owner attestation transport failed") from error
        if status != 200 or len(body) > MAX_MCP_RESPONSE_BYTES:
            raise RunnerError("owner attestation response status/size is invalid")
        try:
            value = json.loads(body.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise RunnerError("owner attestation returned invalid JSON") from error
        if not isinstance(value, dict):
            raise RunnerError("owner attestation returned a non-object")
        return value, body

    def _post(self, payload: dict[str, Any], timeout: float = 900.0) -> Any:
        request = urllib.request.Request(
            f"{self.base_url}/mcp",
            data=json.dumps(payload).encode("utf-8"),
            headers=self._headers(),
            method="POST",
        )
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                session_id = response.headers.get("Mcp-Session-Id")
                if session_id:
                    if self.session_id is not None and session_id != self.session_id:
                        raise RunnerError(
                            "MCP transport attempted to replace its session"
                        )
                    self.session_id = session_id
                body_bytes = response.read(MAX_MCP_RESPONSE_BYTES + 1)
                if len(body_bytes) > MAX_MCP_RESPONSE_BYTES:
                    raise RunnerError(
                        "MCP response exceeds the bounded transport limit"
                    )
                body = body_bytes.decode("utf-8")
        except urllib.error.HTTPError as error:
            detail = error.read(2001).decode("utf-8", errors="replace")[:2000]
            raise RunnerError(f"MCP HTTP {error.code}: {detail}") from error
        except OSError as error:
            raise RunnerError(f"MCP transport error: {error}") from error
        return json.loads(body) if body else None

    def initialize(self) -> None:
        if self.session_id is not None:
            raise RunnerError("MCP client must initialize exactly once")
        self.request_id += 1
        response = self._post(
            {
                "jsonrpc": "2.0",
                "id": self.request_id,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": {"name": self.client_name, "version": "1"},
                },
            },
            timeout=60.0,
        )
        if not isinstance(response, dict) or "result" not in response:
            raise RunnerError("MCP initialize returned no result")
        self._post(
            {
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
                "params": {},
            },
            timeout=60.0,
        )

    def call_tool(self, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        if not self.session_id:
            raise RunnerError("MCP tool call requires the owner's initialized session")
        self.request_id += 1
        response = self._post(
            {
                "jsonrpc": "2.0",
                "id": self.request_id,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments},
            }
        )
        if not isinstance(response, dict):
            raise RunnerError(f"{name} returned no JSON-RPC response")
        result = response.get("result")
        if not isinstance(result, dict):
            raise RunnerError(f"{name} returned no MCP result")
        content = result.get("content")
        text = content[0].get("text") if isinstance(content, list) and content else None
        if result.get("isError"):
            raise RunnerError(f"{name} MCP error: {str(text)[:2000]}")
        if not isinstance(text, str):
            raise RunnerError(f"{name} returned no text content")
        try:
            parsed = json.loads(text)
        except json.JSONDecodeError as error:
            raise RunnerError(f"{name} returned invalid JSON content") from error
        if not isinstance(parsed, dict):
            raise RunnerError(f"{name} returned a non-object result")
        return parsed

    def delete_session(self, timeout: float = 30.0) -> dict[str, Any]:
        """Delete the exact Streamable-HTTP session before owner termination."""
        session_id = self.session_id
        if session_id is None:
            raise RunnerError("cannot delete an uninitialized MCP session")
        self.clear_authorization_lease()
        request = urllib.request.Request(
            f"{self.base_url}/mcp",
            headers=self._headers(),
            method="DELETE",
        )
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                status = getattr(response, "status", response.getcode())
                body = response.read(MAX_MCP_RESPONSE_BYTES + 1)
        except urllib.error.HTTPError as error:
            error.read(2001)
            raise RunnerError(
                f"MCP session DELETE returned HTTP {error.code}"
            ) from error
        except OSError as error:
            raise RunnerError("MCP session DELETE transport failed") from error
        if status not in {200, 202, 204}:
            raise RunnerError(f"MCP session DELETE returned HTTP {status}")
        if len(body) > MAX_MCP_RESPONSE_BYTES:
            raise RunnerError("MCP session DELETE response exceeds transport limit")
        self.session_id = None
        return {
            "session_id": session_id,
            "delete_status": status,
            "session_delete_proven": True,
        }


def _minimal_subprocess_env() -> dict[str, str]:
    allowed = {
        "PATH",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "TMPDIR",
        "TEMP",
        "TMP",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "PATHEXT",
    }
    environment = {key: value for key, value in os.environ.items() if key in allowed}
    environment.setdefault("PATH", os.defpath)
    return environment


def _process_group_popen_kwargs(os_name: str | None = None) -> dict[str, Any]:
    selected = os.name if os_name is None else os_name
    if selected == "nt":
        return {
            "creationflags": getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0x00000200)
        }
    return {"start_new_session": True}


def _terminate_process_group(
    process: subprocess.Popen[Any], *, grace_seconds: float = 5.0
) -> dict[str, Any]:
    """Terminate the exact process tree created for one owner/provider."""
    pid = process.pid
    forced = False
    if os.name == "nt":
        tree_kill = subprocess.run(
            ["taskkill", "/PID", str(pid), "/T", "/F"],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
            timeout=max(grace_seconds, 1.0),
            env=_minimal_subprocess_env(),
        )
        forced = True
        tree_cleanup_proven = tree_kill.returncode == 0
    else:
        tree_cleanup_proven = True
        try:
            os.killpg(pid, 0)
        except ProcessLookupError:
            group_alive = False
        except PermissionError:
            group_alive = True
        else:
            group_alive = True
        if group_alive:
            try:
                os.killpg(pid, signal.SIGTERM)
            except ProcessLookupError:
                group_alive = False
            except PermissionError:
                if process.poll() is None:
                    process.terminate()
        deadline = time.monotonic() + grace_seconds
        while process.poll() is None and time.monotonic() < deadline:
            time.sleep(0.02)
        try:
            os.killpg(pid, 0)
        except ProcessLookupError:
            group_alive = False
        except PermissionError:
            group_alive = True
        else:
            group_alive = True
        if process.poll() is None or group_alive:
            forced = True
            try:
                os.killpg(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            except PermissionError:
                if process.poll() is None:
                    process.kill()
    try:
        process.wait(timeout=max(grace_seconds, 1.0))
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=max(grace_seconds, 1.0))
        forced = True

    if os.name != "nt":
        try:
            os.killpg(pid, 0)
        except ProcessLookupError:
            group_alive = False
        except PermissionError:
            group_alive = True
        else:
            group_alive = True
    else:
        group_alive = process.poll() is None
    return {
        "pid": pid,
        "forced": forced,
        "process_group_terminated": (
            tree_cleanup_proven and process.poll() is not None and not group_alive
        ),
    }


def _bounded_pipe_reader(
    stream: Any,
    limit: int,
    destination: bytearray,
    overflow: threading.Event,
) -> None:
    try:
        while True:
            chunk = stream.read(64 * 1024)
            if not chunk:
                return
            remaining = limit - len(destination)
            if remaining <= 0 or len(chunk) > remaining:
                if remaining > 0:
                    destination.extend(chunk[:remaining])
                overflow.set()
                return
            destination.extend(chunk)
    finally:
        stream.close()


class ExternalAuthorityProvider:
    """Narrow stdin/stdout JSON provider; secrets never enter argv or logs."""

    def __init__(self, executable: pathlib.Path, timeout: float = 30.0) -> None:
        if (
            not isinstance(timeout, (int, float))
            or isinstance(timeout, bool)
            or not math.isfinite(float(timeout))
            or float(timeout) <= 0
            or float(timeout) > MAX_PROVIDER_TIMEOUT_SECONDS
        ):
            raise RunnerError(
                "authority provider timeout must be finite and in (0, 300]"
            )
        executable = pathlib.Path(executable)
        _assert_absolute(executable, "authority provider")
        _assert_no_symlink_components(executable, "authority provider")
        if not executable.is_file() or not os.access(executable, os.X_OK):
            raise RunnerError("authority provider is absent or not executable")
        observed = executable.lstat()
        if not stat.S_ISREG(observed.st_mode):
            raise RunnerError("authority provider is not a regular file")
        self.executable = executable
        self.timeout = float(timeout)
        self._executable_identity = (
            observed.st_dev,
            observed.st_ino,
            observed.st_size,
            observed.st_mtime_ns,
            stat.S_IMODE(observed.st_mode),
        )
        self._identity_digest = _sha256(executable)
        if sys.platform == "darwin" and pathlib.Path("/usr/bin/sandbox-exec").is_file():
            self._blind_boundary_kind = "darwin-sandbox-exec-deny-default-v1"
        elif sys.platform.startswith("linux") and shutil.which("bwrap"):
            self._blind_boundary_kind = "linux-bwrap-unshare-all-v1"
        else:
            self._blind_boundary_kind = "unavailable"

    @property
    def identity_digest(self) -> str:
        return self._identity_digest

    @property
    def blind_boundary_kind(self) -> str:
        return self._blind_boundary_kind

    @property
    def blind_boundary_proven(self) -> bool:
        return self._blind_boundary_kind != "unavailable"

    def _assert_executable_unchanged(self) -> None:
        try:
            observed = self.executable.lstat()
        except OSError as error:
            raise RunnerError(
                "authority provider executable changed after pinning"
            ) from error
        identity = (
            observed.st_dev,
            observed.st_ino,
            observed.st_size,
            observed.st_mtime_ns,
            stat.S_IMODE(observed.st_mode),
        )
        if (
            stat.S_ISLNK(observed.st_mode)
            or not stat.S_ISREG(observed.st_mode)
            or identity != self._executable_identity
            or _sha256(self.executable) != self._identity_digest
        ):
            raise RunnerError("authority provider executable changed after pinning")

    @staticmethod
    def _darwin_sandbox_profile(blind_root: pathlib.Path) -> str:
        allowed_system_roots = (
            "/System",
            "/usr",
            "/bin",
            "/sbin",
            "/Library",
            "/opt/homebrew",
            "/private/etc",
            "/private/var/db/timezone",
            "/dev",
        )
        reads = " ".join(
            f"(subpath {json.dumps(root)})"
            for root in allowed_system_roots
            if pathlib.Path(root).exists()
        )
        root = json.dumps(str(blind_root))
        return (
            "(version 1)\n"
            "(deny default)\n"
            "(allow process*)\n"
            "(allow signal (target self))\n"
            "(allow sysctl-read)\n"
            "(allow mach-lookup)\n"
            f'(allow file-read* (literal "/") (literal "/opt") {reads} '
            f"(subpath {root}))\n"
            f'(allow file-write* (subpath {root}) (literal "/dev/null"))\n'
        )

    def _sandbox_command(
        self, blind_root: pathlib.Path, blind_executable: pathlib.Path
    ) -> list[str]:
        if self._blind_boundary_kind.startswith("darwin-"):
            return [
                "/usr/bin/sandbox-exec",
                "-p",
                self._darwin_sandbox_profile(blind_root),
                str(blind_executable),
            ]
        if self._blind_boundary_kind.startswith("linux-"):
            bwrap = shutil.which("bwrap")
            if bwrap is None:
                raise RunnerError("authority provider blind sandbox disappeared")
            command = [
                bwrap,
                "--die-with-parent",
                "--new-session",
                "--unshare-all",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--tmpfs",
                "/tmp",
            ]
            for root in ("/usr", "/bin", "/lib", "/lib64", "/etc"):
                if pathlib.Path(root).exists():
                    command.extend(("--ro-bind", root, root))
            command.extend(
                (
                    "--ro-bind",
                    str(blind_root),
                    "/work",
                    "--chdir",
                    "/work",
                    "/work/authority-provider",
                )
            )
            return command
        raise RunnerError("authority provider blind filesystem sandbox is unavailable")

    def _invoke(self, request: dict[str, Any]) -> dict[str, Any]:
        self._assert_executable_unchanged()
        if not self.blind_boundary_proven:
            raise RunnerError(
                "authority provider blind filesystem sandbox is unavailable"
            )
        payload = _canonical_bytes(request) + b"\n"
        if len(payload) > MAX_PROVIDER_INPUT_BYTES:
            raise RunnerError("authority provider request exceeds 256 KiB")
        with tempfile.TemporaryDirectory(prefix="m1nd-g6-authority-") as temporary:
            blind_root = pathlib.Path(temporary).resolve()
            blind_root.chmod(0o700)
            blind_executable = blind_root / "authority-provider"
            shutil.copyfile(self.executable, blind_executable, follow_symlinks=False)
            blind_executable.chmod(0o500)
            if _sha256(blind_executable) != self._identity_digest:
                raise RunnerError("authority provider blind copy digest mismatch")
            command = self._sandbox_command(blind_root, blind_executable)
            environment = _minimal_subprocess_env()
            environment["PATH"] = (
                "/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin"
                if sys.platform == "darwin"
                else "/usr/bin:/bin"
            )
            environment["TMPDIR"] = str(blind_root)
            environment["TMP"] = str(blind_root)
            environment["TEMP"] = str(blind_root)
            try:
                process = subprocess.Popen(
                    command,
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    close_fds=True,
                    cwd=blind_root,
                    env=environment,
                    **_process_group_popen_kwargs(),
                )
            except OSError as error:
                raise RunnerError("authority provider could not be executed") from error

            assert process.stdin is not None
            assert process.stdout is not None
            assert process.stderr is not None
            stdout = bytearray()
            stderr = bytearray()
            overflow = threading.Event()
            writer_error: list[BaseException] = []

            def write_request() -> None:
                try:
                    process.stdin.write(payload)
                    process.stdin.flush()
                except (BrokenPipeError, OSError) as error:
                    writer_error.append(error)
                finally:
                    process.stdin.close()

            threads = [
                threading.Thread(target=write_request, daemon=True),
                threading.Thread(
                    target=_bounded_pipe_reader,
                    args=(process.stdout, MAX_PROVIDER_STDOUT_BYTES, stdout, overflow),
                    daemon=True,
                ),
                threading.Thread(
                    target=_bounded_pipe_reader,
                    args=(process.stderr, MAX_PROVIDER_STDERR_BYTES, stderr, overflow),
                    daemon=True,
                ),
            ]
            for thread in threads:
                thread.start()

            deadline = time.monotonic() + self.timeout
            failure: str | None = None
            while True:
                if overflow.is_set():
                    failure = "authority provider output exceeded its bounded limit"
                    break
                if time.monotonic() >= deadline:
                    failure = "authority provider timed out"
                    break
                if process.poll() is not None and all(
                    not thread.is_alive() for thread in threads
                ):
                    break
                time.sleep(0.01)
            cleanup = _terminate_process_group(process, grace_seconds=1.0)
            for thread in threads:
                thread.join(timeout=1.0)
            self._assert_executable_unchanged()
            if not cleanup["process_group_terminated"]:
                raise RunnerError(
                    "authority provider process group cleanup was not proven"
                )
            if failure is not None:
                raise RunnerError(failure)
            if writer_error and process.returncode == 0:
                raise RunnerError(
                    "authority provider did not consume its bounded request"
                )
            if process.returncode != 0:
                raise RunnerError("authority provider refused the request")
            try:
                response = json.loads(bytes(stdout).decode("utf-8"))
            except (UnicodeDecodeError, json.JSONDecodeError) as error:
                raise RunnerError("authority provider returned invalid JSON") from error
            if not isinstance(response, dict):
                raise RunnerError("authority provider returned a non-object response")
            return response

    def preflight(self, request: dict[str, Any]) -> dict[str, Any]:
        return self._invoke(request)

    def authorize(self, request: dict[str, Any]) -> dict[str, Any]:
        return self._invoke(request)


class BinaryAuthorizationReceiptVerifier:
    """Offline verifier executed by the exact already-pinned candidate binary."""

    def __init__(
        self,
        binary: pathlib.Path,
        expected_binary_digest: str,
        timeout: float = 10.0,
    ) -> None:
        binary = pathlib.Path(binary)
        _assert_absolute(binary, "authorization verifier binary")
        _assert_no_symlink_components(binary, "authorization verifier binary")
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise RunnerError(
                "authorization verifier binary is absent or not executable"
            )
        if (
            not isinstance(timeout, (int, float))
            or isinstance(timeout, bool)
            or not math.isfinite(float(timeout))
            or not 0 < float(timeout) <= 60
        ):
            raise RunnerError(
                "authorization verifier timeout must be finite and in (0, 60]"
            )
        self.binary = binary
        self.expected_binary_digest = _require_digest(
            expected_binary_digest,
            "authorization verifier binary digest",
            prefixed=True,
        )
        if _sha256(binary) != self.expected_binary_digest:
            raise RunnerError("authorization verifier binary digest mismatch")
        observed = binary.lstat()
        self._identity = (
            observed.st_dev,
            observed.st_ino,
            observed.st_size,
            observed.st_mtime_ns,
            stat.S_IMODE(observed.st_mode),
        )
        self.timeout = float(timeout)

    def _assert_binary_unchanged(self) -> None:
        try:
            observed = self.binary.lstat()
        except OSError as error:
            raise RunnerError("authorization verifier binary changed") from error
        identity = (
            observed.st_dev,
            observed.st_ino,
            observed.st_size,
            observed.st_mtime_ns,
            stat.S_IMODE(observed.st_mode),
        )
        if (
            stat.S_ISLNK(observed.st_mode)
            or not stat.S_ISREG(observed.st_mode)
            or identity != self._identity
            or _sha256(self.binary) != self.expected_binary_digest
        ):
            raise RunnerError("authorization verifier binary changed")

    def verify(
        self,
        receipt: dict[str, Any],
        key: AuthorityVerificationKey,
        max_future_clock_skew_ms: int,
    ) -> dict[str, Any]:
        self._assert_binary_unchanged()
        request = {
            "schema": AUTHORIZATION_VERIFIER_REQUEST_SCHEMA,
            "receipt": receipt,
            "verification_key": key.as_wire(),
            "max_future_clock_skew_ms": max_future_clock_skew_ms,
        }
        payload = _canonical_bytes(request) + b"\n"
        if len(payload) > MAX_PROVIDER_INPUT_BYTES:
            raise RunnerError("authorization verifier request exceeds 256 KiB")
        try:
            process = subprocess.Popen(
                [str(self.binary), "--verify-authorization-receipt"],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                close_fds=True,
                env=_minimal_subprocess_env(),
                **_process_group_popen_kwargs(),
            )
        except OSError as error:
            raise RunnerError("authorization verifier could not be executed") from error
        assert process.stdin is not None
        assert process.stdout is not None
        assert process.stderr is not None
        stdout = bytearray()
        stderr = bytearray()
        overflow = threading.Event()
        writer_error: list[BaseException] = []

        def write_request() -> None:
            try:
                process.stdin.write(payload)
                process.stdin.flush()
            except (BrokenPipeError, OSError) as error:
                writer_error.append(error)
            finally:
                process.stdin.close()

        threads = [
            threading.Thread(target=write_request, daemon=True),
            threading.Thread(
                target=_bounded_pipe_reader,
                args=(process.stdout, MAX_PROVIDER_STDOUT_BYTES, stdout, overflow),
                daemon=True,
            ),
            threading.Thread(
                target=_bounded_pipe_reader,
                args=(process.stderr, MAX_PROVIDER_STDERR_BYTES, stderr, overflow),
                daemon=True,
            ),
        ]
        for thread in threads:
            thread.start()
        deadline = time.monotonic() + self.timeout
        failure: str | None = None
        while True:
            if overflow.is_set():
                failure = "authorization verifier output exceeded its bounded limit"
                break
            if time.monotonic() >= deadline:
                failure = "authorization verifier timed out"
                break
            if process.poll() is not None and all(
                not thread.is_alive() for thread in threads
            ):
                break
            time.sleep(0.01)
        cleanup = _terminate_process_group(process, grace_seconds=1.0)
        for thread in threads:
            thread.join(timeout=1.0)
        self._assert_binary_unchanged()
        if not cleanup["process_group_terminated"]:
            raise RunnerError("authorization verifier process cleanup was not proven")
        if failure is not None:
            raise RunnerError(failure)
        if writer_error and process.returncode == 0:
            raise RunnerError("authorization verifier did not consume its request")
        if process.returncode != 0:
            raise RunnerError("authorization receipt cryptographic verification failed")
        try:
            response = json.loads(bytes(stdout).decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise RunnerError("authorization verifier returned invalid JSON") from error
        if not isinstance(response, dict):
            raise RunnerError("authorization verifier returned a non-object response")
        return response


def preflight_authority_provider(
    provider: AuthorityProvider | None,
    trusted_assembly: AuthorityAssembly,
    *,
    diagnostic: bool,
    lane: str,
    binary_digest: str,
) -> AuthorityAssembly:
    """Prove provider/assembly presence before owners or measurements start."""
    if provider is None:
        mode = "formal" if not diagnostic else "diagnostic"
        raise RunnerError(f"external authority provider is required for {mode} runs")
    if not trusted_assembly.expected_digest_verified:
        raise RunnerError("authority assembly lacks an independently pinned digest")
    if trusted_assembly.binary_digest != binary_digest:
        raise RunnerError("authority assembly is bound to a foreign owner binary")
    if provider.identity_digest != trusted_assembly.provider_executable_digest:
        raise RunnerError(
            "authority provider executable differs from the pinned assembly"
        )
    if (
        provider.blind_boundary_kind != trusted_assembly.blind_boundary_kind
        or provider.blind_boundary_proven != trusted_assembly.blind_boundary_proven
    ):
        raise RunnerError("authority provider blind-boundary identity drifted")
    if not diagnostic and not trusted_assembly.blind_boundary_proven:
        raise RunnerError("formal authority provider requires a proven blind sandbox")
    request_id = f"g6-authority-preflight-{lane}-{binary_digest[-12:]}"
    request = {
        "schema": AUTHORITY_PREFLIGHT_REQUEST_SCHEMA,
        "request_id": request_id,
        "operation": "preflight",
        "mode": "diagnostic" if diagnostic else "formal",
        "lane": lane,
        "binary_digest": binary_digest,
        "provider_executable_digest": provider.identity_digest,
        "assembly_digest": trusted_assembly.assembly_digest,
    }
    response = _require_exact_fields(
        provider.preflight(request),
        frozenset(
            {
                "schema",
                "request_id",
                "provider_kind",
                "production_authority_assembly",
                "assembly_id",
                "assembly_digest",
                "binary_digest",
                "provider_executable_digest",
            }
        ),
        "authority preflight response",
    )
    if response["schema"] != AUTHORITY_PREFLIGHT_RESPONSE_SCHEMA:
        raise RunnerError("authority preflight response schema mismatch")
    if response["request_id"] != request_id:
        raise RunnerError("authority preflight response request binding mismatch")
    provider_kind = response["provider_kind"]
    if provider_kind not in {"production", "software_test"}:
        raise RunnerError("authority provider kind is invalid")
    if not isinstance(response["production_authority_assembly"], bool):
        raise RunnerError("authority assembly flag must be boolean")
    provider_binary = _require_digest(
        response["provider_executable_digest"],
        "authority provider executable digest",
        prefixed=True,
    )
    if (
        provider_kind != trusted_assembly.provider_kind
        or response["production_authority_assembly"]
        is not trusted_assembly.production_authority_assembly
        or response["assembly_id"] != trusted_assembly.assembly_id
        or response["assembly_digest"] != trusted_assembly.assembly_digest
        or response["binary_digest"] != trusted_assembly.binary_digest
        or provider_binary != trusted_assembly.provider_executable_digest
    ):
        raise RunnerError("authority provider preflight differs from pinned assembly")
    if not diagnostic and not trusted_assembly.production_authority_assembly:
        raise RunnerError("formal run requires a pinned production authority assembly")
    return trusted_assembly


def _validate_graph_preview(
    spec: OwnerSpec,
    session_id: str,
    request_id: str,
    preview: dict[str, Any],
) -> None:
    _require_exact_fields(preview, PREVIEW_FIELDS, "graph-ingest preview")
    if preview["schema"] != GRAPH_PREVIEW_RESPONSE_SCHEMA:
        raise RunnerError("graph-ingest preview schema mismatch")
    if preview["request_id"] != request_id:
        raise RunnerError("graph-ingest preview request binding mismatch")
    if preview["semantic_action"] != "graph.ingest.replace":
        raise RunnerError("graph-ingest preview selected a foreign semantic action")
    effects = preview["requested_effects"]
    if not isinstance(effects, list) or len(effects) != len(set(effects)):
        raise RunnerError("graph-ingest preview effects are malformed")
    if frozenset(effects) != GRAPH_INGEST_EFFECTS:
        raise RunnerError(
            "graph-ingest preview effects differ from the closed contract"
        )
    if (
        preview["authority_floor"] != "POSITIVE_SOVEREIGN"
        or preview["risk_class"] != "CRITICAL"
        or preview["ingress"] != "MCP"
    ):
        raise RunnerError("graph-ingest preview authority/risk/ingress mismatch")
    if preview["transport_session_id"] != session_id:
        raise RunnerError("graph-ingest preview is bound to a foreign MCP session")
    if not _same_root(preview["root_identity"], spec.root):
        raise RunnerError("graph-ingest preview is bound to a foreign source root")
    if preview["route_selector"] is not None and not _same_root(
        preview["route_selector"], spec.root
    ):
        raise RunnerError("graph-ingest preview route selector is outside owner scope")
    _require_nonempty_string(preview["actor_brain_id"], "preview actor_brain_id")
    _require_nonempty_string(preview["scan_job_id"], "preview scan_job_id")
    for field in (
        "preview_id",
        "ingress_context_digest",
        "expected_source_projection_digest",
        "candidate_ownership_digest",
        "candidate_source_projection_digest",
        "candidate_pipeline_digest",
        "semantic_payload_digest",
        "operation_object_digest",
    ):
        _require_digest(preview[field], f"preview {field}", prefixed=False)
    if (
        not isinstance(preview["expected_graph_generation"], int)
        or preview["expected_graph_generation"] < 0
    ):
        raise RunnerError("graph-ingest preview graph generation is invalid")

    binding = _require_exact_fields(
        preview["authority_binding"],
        frozenset(
            {
                "target_action",
                "payload_digest",
                "requested_effects",
                "mission_id",
                "mission_head_id",
            }
        ),
        "preview authority binding",
    )
    if (
        binding["target_action"] != preview["semantic_action"]
        or binding["payload_digest"] != preview["operation_object_digest"]
        or binding["requested_effects"] != effects
        or binding["mission_id"] is not None
        or binding["mission_head_id"] is not None
    ):
        raise RunnerError("preview authority binding does not seal the preview")

    execute = _require_exact_fields(
        preview["execute_request"],
        frozenset({"action", "schema", "request_id", "request"}),
        "preview execute request",
    )
    if (
        execute["action"] != "graph_ingest_replace"
        or execute["schema"] != EXTERNAL_MUTATION_REQUEST_SCHEMA
        or execute["request_id"] != request_id
    ):
        raise RunnerError("preview execute request is not the exact replace request")
    ingest_request = _require_exact_fields(
        execute["request"],
        frozenset(
            {
                "preview_id",
                "root",
                "expected_graph_generation",
                "expected_source_projection_digest",
                "include_dotfiles",
                "dotfile_patterns",
                "parent",
            }
        ),
        "preview graph-ingest request",
    )
    if (
        ingest_request["preview_id"] != preview["preview_id"]
        or not _same_root(ingest_request["root"], spec.root)
        or ingest_request["expected_graph_generation"]
        != preview["expected_graph_generation"]
        or ingest_request["expected_source_projection_digest"]
        != preview["expected_source_projection_digest"]
        or ingest_request["include_dotfiles"] is not False
        or ingest_request["dotfile_patterns"] != []
        or ingest_request["parent"] is not None
    ):
        raise RunnerError(
            "preview execute request changed the closed source projection"
        )


def _authority_provider_request(
    spec: OwnerSpec,
    preview: dict[str, Any],
    assembly: AuthorityAssembly,
    *,
    diagnostic: bool,
    lane: str,
    binary_digest: str,
) -> dict[str, Any]:
    return {
        "schema": AUTHORITY_PROVIDER_REQUEST_SCHEMA,
        "request_id": f"g6-provider-{lane}-{spec.owner_id}",
        "operation": "authorize_graph_ingest",
        "mode": "diagnostic" if diagnostic else "formal",
        "assembly": {
            "provider_kind": assembly.provider_kind,
            "production_authority_assembly": assembly.production_authority_assembly,
            "assembly_id": assembly.assembly_id,
            "assembly_digest": assembly.assembly_digest,
        },
        "owner": {
            "owner_id": spec.owner_id,
            "repo_id": spec.repo_id,
            "scope": spec.scope,
            "port": spec.port,
            "source_revision": spec.source_revision,
            "file_set_digest": spec.file_set_digest,
            "binary_digest": binary_digest,
        },
        "preview": {
            "preview_id": preview["preview_id"],
            "semantic_action": preview["semantic_action"],
            "requested_effects": preview["requested_effects"],
            "actor_brain_id": preview["actor_brain_id"],
            "transport_session_id": preview["transport_session_id"],
            "ingress_context_digest": preview["ingress_context_digest"],
            "root_identity": preview["root_identity"],
            "semantic_payload_digest": preview["semantic_payload_digest"],
            "operation_object_digest": preview["operation_object_digest"],
            "authority_binding": preview["authority_binding"],
        },
    }


def _validate_provider_authorization(
    response: dict[str, Any],
    request: dict[str, Any],
    spec: OwnerSpec,
    preview: dict[str, Any],
    assembly: AuthorityAssembly,
    binary_digest: str,
) -> dict[str, Any]:
    response = _require_exact_fields(
        response,
        frozenset(
            {
                "schema",
                "request_id",
                "provider_kind",
                "assembly_id",
                "assembly_digest",
                "owner_id",
                "repo_id",
                "scope",
                "source_revision",
                "file_set_digest",
                "binary_digest",
                "preview_id",
                "transport_session_id",
                "ingress_context_digest",
                "operation_object_digest",
                "authorization_request",
            }
        ),
        "authority provider response",
    )
    expected = {
        "schema": AUTHORITY_PROVIDER_RESPONSE_SCHEMA,
        "request_id": request["request_id"],
        "provider_kind": assembly.provider_kind,
        "assembly_id": assembly.assembly_id,
        "assembly_digest": assembly.assembly_digest,
        "owner_id": spec.owner_id,
        "repo_id": spec.repo_id,
        "scope": spec.scope,
        "source_revision": spec.source_revision,
        "file_set_digest": spec.file_set_digest,
        "binary_digest": binary_digest,
        "preview_id": preview["preview_id"],
        "transport_session_id": preview["transport_session_id"],
        "ingress_context_digest": preview["ingress_context_digest"],
        "operation_object_digest": preview["operation_object_digest"],
    }
    for field, value in expected.items():
        if response[field] != value:
            raise RunnerError(f"authority provider returned a foreign {field} binding")

    authorization = _require_exact_fields(
        response["authorization_request"],
        AUTHORIZATION_REQUEST_FIELDS,
        "provider authorization request",
    )
    binding = preview["authority_binding"]
    if (
        authorization["schema"] != AUTHORITY_AUTHORIZE_REQUEST_SCHEMA
        or not isinstance(authorization["request_id"], str)
        or not authorization["request_id"].strip()
        or not isinstance(authorization["authority_session_id"], str)
        or not authorization["authority_session_id"].strip()
        or authorization["authority_session_context_digest"]
        != preview["ingress_context_digest"]
        or authorization["target_action"] != binding["target_action"]
        or authorization["payload_digest"] != binding["payload_digest"]
        or authorization["requested_effects"] != binding["requested_effects"]
        or authorization["mission_id"] != binding["mission_id"]
        or authorization["mission_head_id"] != binding["mission_head_id"]
    ):
        raise RunnerError("provider authorization does not bind the exact preview")
    input_value = authorization["input"]
    if (
        not isinstance(input_value, dict)
        or input_value.get("authority") != "positive_sovereign"
    ):
        raise RunnerError("graph ingest requires positive-sovereign provider authority")
    return authorization


def _validate_authorization_response(
    response: dict[str, Any],
    authorization_request: dict[str, Any],
    preview: dict[str, Any],
    assembly: AuthorityAssembly,
    verifier: AuthorizationReceiptVerifier | None,
    *,
    diagnostic: bool,
) -> tuple[str, dict[str, Any]]:
    response = _require_exact_fields(
        response,
        frozenset(
            {
                "schema",
                "request_id",
                "authorization_lease_id",
                "authorization_receipt",
                "expires_at",
            }
        ),
        "authority authorize response",
    )
    if (
        response["schema"] != AUTHORITY_AUTHORIZE_RESPONSE_SCHEMA
        or response["request_id"] != authorization_request["request_id"]
    ):
        raise RunnerError("authority response request binding mismatch")
    lease = _require_digest(
        response["authorization_lease_id"], "authorization lease", prefixed=False
    )
    if not isinstance(response["expires_at"], int) or response["expires_at"] <= 0:
        raise RunnerError("authorization lease expiry is invalid")
    receipt = _require_exact_fields(
        response["authorization_receipt"],
        AUTHORIZATION_RECEIPT_FIELDS,
        "authorization receipt",
    )
    if receipt["schema"] != AUTHORIZATION_RECEIPT_SCHEMA:
        raise RunnerError("authorization receipt schema mismatch")
    core = _require_exact_fields(
        receipt["core"], AUTHORIZATION_RECEIPT_CORE_FIELDS, "authorization receipt core"
    )
    if (
        core.get("brain_id") != preview["actor_brain_id"]
        or core.get("verified_object_digest") != preview["operation_object_digest"]
        or core.get("transport_session_id") != preview["transport_session_id"]
        or core.get("ingress_context_digest") != preview["ingress_context_digest"]
        or core.get("action") != preview["semantic_action"]
        or core.get("ingress") != "MCP"
        or core.get("complete_effects") != preview["requested_effects"]
        or core.get("mission_id") != preview["authority_binding"]["mission_id"]
        or core.get("mission_head_id")
        != preview["authority_binding"]["mission_head_id"]
        or core.get("expires_at") != response["expires_at"]
    ):
        raise RunnerError(
            "authorization receipt is foreign to owner/session/digest/scope"
        )
    _require_digest(
        receipt["receipt_digest"], "authorization receipt digest", prefixed=False
    )
    expected_receipt_digest = _rust_domain_digest(
        AUTHORIZATION_RECEIPT_DIGEST_DOMAIN, core
    )
    if receipt["receipt_digest"] != expected_receipt_digest:
        raise RunnerError("authorization receipt digest does not bind its exact core")

    for field in (
        "organism_id",
        "repo_id",
        "subject_id",
        "role",
        "capability_id",
        "active_mode",
    ):
        _require_nonempty_string(core[field], f"authorization receipt {field}")
    for field in (
        "constitution_digest",
        "policy_registry_digest",
        "authority_body_digest",
        "journal_root_digest",
    ):
        _require_digest(core[field], f"authorization receipt {field}", prefixed=False)
    for field in (
        "constitution_epoch",
        "autonomy_epoch",
        "protected_epoch_at_decision",
        "replay_sequence",
        "journal_sequence",
        "protected_epoch",
        "authorized_at",
        "expires_at",
    ):
        if (
            not isinstance(core[field], int)
            or isinstance(core[field], bool)
            or core[field] < 0
        ):
            raise RunnerError(f"authorization receipt {field} is invalid")
    if core["authorized_at"] >= core["expires_at"]:
        raise RunnerError("authorization receipt time window is invalid")
    if core["expires_at"] - core["authorized_at"] > MAX_AUTHORIZATION_LEASE_MS:
        raise RunnerError("authorization receipt lifetime exceeds five minutes")
    policy_tuple = _require_exact_fields(
        core["exact_policy_tuple"],
        EXACT_POLICY_TUPLE_FIELDS,
        "authorization receipt exact policy tuple",
    )
    decision_digest = core["authority_decision_digest"]
    if decision_digest is not None:
        _require_digest(
            decision_digest, "authorization authority decision", prefixed=False
        )

    authority = core["authority"]
    production_variant: str | None = None
    production_assurance = False
    if isinstance(authority, dict) and set(authority) == {"POSITIVE"}:
        positive = _require_exact_fields(
            authority["POSITIVE"],
            frozenset({"variant", "assurance"}),
            "positive authorization authority",
        )
        production_variant = positive["variant"]
        production_assurance = (
            production_variant in {"HUMAN", "POLICY", "AGENT_QUORUM"}
            and positive["assurance"] == "CONTROL_VERIFIED_ED25519"
        )
    elif isinstance(authority, dict) and set(authority) == {"AUTONOMOUS"}:
        autonomous = _require_exact_fields(
            authority["AUTONOMOUS"],
            frozenset({"variant", "capability_assurance", "admission_receipt_digest"}),
            "autonomous authorization authority",
        )
        production_variant = autonomous["variant"]
        _require_digest(
            autonomous["admission_receipt_digest"],
            "autonomous admission receipt digest",
            prefixed=False,
        )
        production_assurance = (
            production_variant in {"POLICY", "AGENT_QUORUM"}
            and autonomous["capability_assurance"] == "CONTROL_VERIFIED_ED25519"
        )
    if (
        policy_tuple["ingress"] != "MCP"
        or policy_tuple["action"] != preview["semantic_action"]
        or policy_tuple["active_mode"] != core["active_mode"]
        or policy_tuple["subject_id"] != core["subject_id"]
        or policy_tuple["authority_variant"] != production_variant
        or policy_tuple["risk_class"] != "CRITICAL"
    ):
        raise RunnerError("authorization receipt exact policy tuple is foreign")

    issuer = _require_nonempty_string(receipt["issuer"], "receipt issuer")
    key_id = _require_nonempty_string(receipt["key_id"], "receipt key_id")
    algorithm = _require_nonempty_string(receipt["algorithm"], "receipt algorithm")
    signature = _require_nonempty_string(receipt["signature"], "receipt signature")
    key = assembly.verification_key
    signer_metadata_bound = (
        issuer == key.subject_id
        and key_id == key.key_id
        and algorithm == key.algorithm
        and algorithm == "ED25519"
        and re.fullmatch(r"[0-9a-f]{128}", signature) is not None
    )
    verifier_proof: dict[str, Any] | None = None
    signature_verified = False
    key_lifecycle_verified = False
    checked_at_ms = int(time.time_ns() // 1_000_000)
    if signer_metadata_bound and assembly.production_authority_assembly:
        if verifier is None:
            if not diagnostic:
                raise RunnerError(
                    "formal receipt verification requires the candidate binary"
                )
        else:
            verifier_proof = _require_exact_fields(
                verifier.verify(receipt, key, assembly.max_future_clock_skew_ms),
                frozenset(
                    {
                        "schema",
                        "status",
                        "checked_at_ms",
                        "receipt_digest",
                        "issuer",
                        "key_id",
                        "algorithm",
                        "signature_verified",
                        "clock_verified",
                        "key_lifecycle_verified",
                    }
                ),
                "authorization verifier proof",
            )
            checked_at_ms = _require_u64(
                verifier_proof["checked_at_ms"], "authorization verifier check time"
            )
            if (
                verifier_proof["schema"] != AUTHORIZATION_VERIFIER_RESPONSE_SCHEMA
                or verifier_proof["status"] != "VERIFIED"
                or verifier_proof["receipt_digest"] != receipt["receipt_digest"]
                or verifier_proof["issuer"] != issuer
                or verifier_proof["key_id"] != key_id
                or verifier_proof["algorithm"] != algorithm
                or verifier_proof["signature_verified"] is not True
                or verifier_proof["clock_verified"] is not True
                or verifier_proof["key_lifecycle_verified"] is not True
            ):
                raise RunnerError(
                    "authorization verifier proof is foreign or incomplete"
                )
            signature_verified = True
            key_lifecycle_verified = True
    clock_verified = bool(
        core["authorized_at"] <= checked_at_ms < core["expires_at"]
        and response["expires_at"] == core["expires_at"]
    )
    if not clock_verified:
        raise RunnerError("authorization receipt is future-dated or expired")
    if verifier_proof is not None and verifier_proof["clock_verified"] is not True:
        raise RunnerError("authorization verifier did not prove the receipt clock")
    signer_production = bool(
        signer_metadata_bound and signature_verified and key_lifecycle_verified
    )
    production_proven = bool(
        production_assurance
        and signer_production
        and clock_verified
        and assembly.production_authority_assembly
        and assembly.expected_digest_verified
    )
    if not diagnostic and not production_proven:
        raise RunnerError(
            "formal graph ingest requires a production authority receipt; provider claims are insufficient"
        )
    return lease, {
        "authority_variant": production_variant,
        "control_verified_ed25519": production_assurance,
        "receipt_core_digest_verified": True,
        "assembly_digest_verified": assembly.expected_digest_verified,
        "key_registry_epoch": assembly.key_registry_epoch,
        "signature_verified": signature_verified,
        "clock_verified": clock_verified,
        "key_lifecycle_verified": key_lifecycle_verified,
        "checked_at_ms": checked_at_ms,
        "receipt_signer_metadata_production": signer_production,
        "production_authority_receipt_proven": production_proven,
        "receipt_digest": receipt["receipt_digest"],
        "issuer": issuer,
        "key_id": key_id,
        "algorithm": algorithm,
    }


def _require_nonnegative_integer(value: Any, label: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        raise RunnerError(f"{label} must be a non-negative integer")
    return value


def _serde_json_bytes(value: Any) -> bytes:
    """Match serde_json::to_vec for the explicitly ordered typed projections below."""
    try:
        return json.dumps(
            value,
            ensure_ascii=False,
            separators=(",", ":"),
            sort_keys=False,
            allow_nan=False,
        ).encode("utf-8")
    except (TypeError, ValueError) as error:
        raise RunnerError("typed Rust JSON projection is not serializable") from error


def _serde_sha256(value: Any) -> str:
    return hashlib.sha256(_serde_json_bytes(value)).hexdigest()


def _ordered_claimed_edge(value: dict[str, Any]) -> dict[str, Any]:
    return {
        "source": value["source"],
        "target": value["target"],
        "relation": value["relation"],
        "direction": value["direction"],
        "inhibitory": value["inhibitory"],
    }


def _ordered_resolution_input(value: dict[str, Any]) -> dict[str, Any]:
    return {
        "source_key": value["source_key"],
        "source_id": value["source_id"],
        "target_label": value["target_label"],
        "relation": value["relation"],
    }


def _ordered_resolution_hint(value: dict[str, Any]) -> dict[str, Any]:
    return {
        "source_id": value["source_id"],
        "target_label": value["target_label"],
        "import_path": value["import_path"],
    }


def _ordered_resolution_decision(value: dict[str, Any]) -> dict[str, Any]:
    return {
        "source_key": value["source_key"],
        "source_id": value["source_id"],
        "target_label": value["target_label"],
        "relation": value["relation"],
        "outcome": value["outcome"],
        "resolved_target_id": value["resolved_target_id"],
        "candidate_ids": value["candidate_ids"],
        "source_line_start": value["source_line_start"],
        "source_line_end": value["source_line_end"],
    }


def _ordered_pipeline_receipt(value: dict[str, Any]) -> dict[str, Any]:
    return {field_name: value[field_name] for field_name in PIPELINE_RECEIPT_ORDER}


def _ordered_source_digests(value: dict[str, Any]) -> dict[str, Any]:
    return {source: value[source] for source in sorted(value)}


def _ordered_claims_by_source(value: dict[str, Any]) -> dict[str, Any]:
    return {
        source: {
            "source_hint": value[source]["source_hint"],
            "node_ids": value[source]["node_ids"],
            "edges": [_ordered_claimed_edge(edge) for edge in value[source]["edges"]],
        }
        for source in sorted(value)
    }


def _recomputed_ownership_digests(ownership: dict[str, Any]) -> dict[str, str]:
    resolution_inputs = [
        _ordered_resolution_input(row) for row in ownership["resolution_inputs"]
    ]
    resolution_hints = [
        _ordered_resolution_hint(row) for row in ownership["resolution_hints"]
    ]
    resolution_decisions = [
        _ordered_resolution_decision(row) for row in ownership["resolution_decisions"]
    ]
    pipeline_receipt = _ordered_pipeline_receipt(ownership["pipeline_receipt"])
    source_digests = _ordered_source_digests(ownership["source_digests"])
    claims_by_source = _ordered_claims_by_source(ownership["claims_by_source"])
    resolution_input_digest = _serde_sha256(
        [CODE_RESOLUTION_INPUT_DIGEST_DOMAIN, resolution_inputs]
    )
    resolution_hint_digest = _serde_sha256(
        [CODE_RESOLUTION_HINT_DIGEST_DOMAIN, resolution_hints]
    )
    resolution_digest = _serde_sha256(
        [CODE_RESOLUTION_DIGEST_DOMAIN, resolution_decisions]
    )
    pipeline_digest = _serde_sha256([CODE_PIPELINE_DIGEST_DOMAIN, pipeline_receipt])
    lineage_digest = _serde_sha256(
        [
            CODE_LINEAGE_DIGEST_DOMAIN,
            ownership["root_identity"],
            ownership["exact_source_key"],
            ownership["base_ownership_digest"],
            source_digests,
        ]
    )
    edge_list_fields = (
        "unowned_edges",
        "dangling_edge_claims",
        "duplicate_graph_edges",
    )
    ownership_projection = {
        "domain": CODE_OWNERSHIP_DIGEST_DOMAIN,
        "root_identity": ownership["root_identity"],
        "exact_source_key": ownership["exact_source_key"],
        "base_ownership_digest": ownership["base_ownership_digest"],
        "source_digests": source_digests,
        "claims_by_source": claims_by_source,
        "source_projection_digest": ownership["source_projection_digest"],
        "graph_finalized": ownership["graph_finalized"],
        "pending_edge_count": ownership["pending_edge_count"],
        "bidirectional_mirrors_valid": ownership["bidirectional_mirrors_valid"],
        "csr_shape_valid": ownership["csr_shape_valid"],
        "reverse_csr_valid": ownership["reverse_csr_valid"],
        "orphan_node_slots": ownership["orphan_node_slots"],
        "multiply_identified_node_slots": ownership["multiply_identified_node_slots"],
        "invalid_identity_ids": ownership["invalid_identity_ids"],
        "out_of_range_identity_ids": ownership["out_of_range_identity_ids"],
        "orphan_edge_slots": ownership["orphan_edge_slots"],
        "resolution_inputs": resolution_inputs,
        "resolution_input_digest": resolution_input_digest,
        "resolution_hints": resolution_hints,
        "resolution_hint_digest": resolution_hint_digest,
        "resolution_decisions": resolution_decisions,
        "resolution_digest": resolution_digest,
        "pipeline_receipt": pipeline_receipt,
        "pipeline_digest": pipeline_digest,
        "coverage": ownership["coverage"],
        "unowned_nodes": ownership["unowned_nodes"],
        **{
            field_name: [_ordered_claimed_edge(edge) for edge in ownership[field_name]]
            for field_name in edge_list_fields
        },
        "dangling_node_claims": ownership["dangling_node_claims"],
    }
    # Rust field order places dangling_node_claims before the two final edge lists.
    ownership_projection = {
        "domain": ownership_projection["domain"],
        "root_identity": ownership_projection["root_identity"],
        "exact_source_key": ownership_projection["exact_source_key"],
        "base_ownership_digest": ownership_projection["base_ownership_digest"],
        "source_digests": ownership_projection["source_digests"],
        "claims_by_source": ownership_projection["claims_by_source"],
        "source_projection_digest": ownership_projection["source_projection_digest"],
        "graph_finalized": ownership_projection["graph_finalized"],
        "pending_edge_count": ownership_projection["pending_edge_count"],
        "bidirectional_mirrors_valid": ownership_projection[
            "bidirectional_mirrors_valid"
        ],
        "csr_shape_valid": ownership_projection["csr_shape_valid"],
        "reverse_csr_valid": ownership_projection["reverse_csr_valid"],
        "orphan_node_slots": ownership_projection["orphan_node_slots"],
        "multiply_identified_node_slots": ownership_projection[
            "multiply_identified_node_slots"
        ],
        "invalid_identity_ids": ownership_projection["invalid_identity_ids"],
        "out_of_range_identity_ids": ownership_projection["out_of_range_identity_ids"],
        "orphan_edge_slots": ownership_projection["orphan_edge_slots"],
        "resolution_inputs": ownership_projection["resolution_inputs"],
        "resolution_input_digest": ownership_projection["resolution_input_digest"],
        "resolution_hints": ownership_projection["resolution_hints"],
        "resolution_hint_digest": ownership_projection["resolution_hint_digest"],
        "resolution_decisions": ownership_projection["resolution_decisions"],
        "resolution_digest": ownership_projection["resolution_digest"],
        "pipeline_receipt": ownership_projection["pipeline_receipt"],
        "pipeline_digest": ownership_projection["pipeline_digest"],
        "coverage": ownership_projection["coverage"],
        "unowned_nodes": ownership_projection["unowned_nodes"],
        "unowned_edges": ownership_projection["unowned_edges"],
        "dangling_node_claims": ownership_projection["dangling_node_claims"],
        "dangling_edge_claims": ownership_projection["dangling_edge_claims"],
        "duplicate_graph_edges": ownership_projection["duplicate_graph_edges"],
    }
    return {
        "resolution_input_digest": resolution_input_digest,
        "resolution_hint_digest": resolution_hint_digest,
        "resolution_digest": resolution_digest,
        "pipeline_digest": pipeline_digest,
        "lineage_digest": lineage_digest,
        "ownership_digest": _serde_sha256(ownership_projection),
    }


def _validate_pipeline_receipt(
    value: Any,
    *,
    source_count: int,
    source_paths: tuple[str, ...],
    binary_digest: str,
    candidate_pipeline_digest: str,
    manifest_pipeline_digest: Any,
) -> dict[str, Any]:
    receipt = _require_exact_fields(
        value, PIPELINE_RECEIPT_FIELDS, "code pipeline receipt"
    )
    if (
        receipt["schema"] != CODE_PIPELINE_RECEIPT_SCHEMA
        or receipt["producer_name"] != "m1nd-ingest"
        or receipt["include_dotfiles"] is not False
        or receipt["dotfile_patterns"] != []
        or receipt["immutable_source_snapshot"] is not True
        or receipt["global_enrichment_enabled"] is not True
    ):
        raise RunnerError(
            "code pipeline receipt mode/policy is not the full-root contract"
        )
    for field in (
        "pipeline_version",
        "producer_version",
        "binary_policy",
    ):
        _require_nonempty_string(receipt[field], f"pipeline receipt {field}")
    for field in (
        "producer_build_identity",
        "producer_executable_identity",
        "policy_fingerprint",
        "vcs_context_digest",
    ):
        _require_digest(receipt[field], f"pipeline receipt {field}", prefixed=False)
    if receipt["producer_executable_identity"] != binary_digest.removeprefix("sha256:"):
        raise RunnerError("pipeline receipt was produced by a foreign executable")
    for field in ("skip_dirs", "skip_files", "build_features"):
        if not isinstance(receipt[field], list) or any(
            not isinstance(item, str) for item in receipt[field]
        ):
            raise RunnerError(f"pipeline receipt {field} is malformed")
    for field in (
        "discovered_source_count",
        "extracted_source_count",
        "digested_source_count",
    ):
        if (
            _require_nonnegative_integer(receipt[field], f"pipeline receipt {field}")
            != source_count
        ):
            raise RunnerError(
                "pipeline receipt does not cover the exact source file set"
            )
    cross_file = [
        _require_nonnegative_integer(receipt[field], f"pipeline receipt {field}")
        for field in (
            "cross_file_source_files_expected",
            "cross_file_source_metadata_verified",
            "cross_file_source_files_read",
            "cross_file_source_files_parsed",
        )
    ]
    expected_cross_file_count = sum(
        path.rsplit(".", 1)[-1] in CROSS_FILE_SOURCE_EXTENSIONS
        for path in source_paths
        if "." in path
    )
    if len(set(cross_file)) != 1 or cross_file[0] != expected_cross_file_count:
        raise RunnerError("pipeline receipt cross-file accounting is incomplete")
    for expected_field, accounted_field in (
        ("cargo_workspace_members_expected", "cargo_workspace_members_accounted"),
        ("cargo_dependency_inputs_expected", "cargo_dependency_inputs_accounted"),
        ("cargo_package_file_links_expected", "cargo_package_file_links_accounted"),
    ):
        expected = _require_nonnegative_integer(
            receipt[expected_field], f"pipeline receipt {expected_field}"
        )
        accounted = _require_nonnegative_integer(
            receipt[accounted_field], f"pipeline receipt {accounted_field}"
        )
        if expected != accounted:
            raise RunnerError("pipeline receipt cargo accounting is incomplete")
    _require_digest(
        manifest_pipeline_digest, "ownership pipeline digest", prefixed=False
    )
    recomputed_pipeline_digest = _serde_sha256(
        [CODE_PIPELINE_DIGEST_DOMAIN, _ordered_pipeline_receipt(receipt)]
    )
    if (
        manifest_pipeline_digest != candidate_pipeline_digest
        or manifest_pipeline_digest != recomputed_pipeline_digest
    ):
        raise RunnerError(
            "ownership pipeline digest does not bind the exact pipeline receipt"
        )
    return {
        "pipeline_receipt_exact": True,
        "producer_executable_bound": True,
        "source_count_bound": source_count,
    }


def _validate_claimed_edge(value: Any, label: str) -> None:
    edge = _require_exact_fields(value, CLAIMED_EDGE_FIELDS, label)
    for field in ("source", "target", "relation"):
        _require_nonempty_string(edge[field], f"{label} {field}")
    if edge["direction"] not in {0, 1} or not isinstance(edge["inhibitory"], bool):
        raise RunnerError(f"{label} direction/inhibitory fields are invalid")
    if edge["direction"] == 1 and edge["source"] > edge["target"]:
        raise RunnerError(f"{label} bidirectional identity is not canonical")
    if any(not _valid_external_id(edge[field]) for field in ("source", "target")):
        raise RunnerError(f"{label} contains an invalid external identity")
    if edge["relation"] != edge["relation"].strip():
        raise RunnerError(f"{label} relation is not canonical")


def _valid_external_id(value: Any) -> bool:
    if (
        not isinstance(value, str)
        or not value
        or value != value.strip()
        or "\x00" in value
        or "\ufffd" in value
    ):
        return False
    if value.startswith("file::"):
        try:
            _canonical_relative_posix(value.removeprefix("file::"), "external file id")
        except RunnerError:
            return False
    return True


def _strictly_increasing(values: list[Any], *, key: Any = None) -> bool:
    projection = [key(value) for value in values] if key is not None else values
    return all(left < right for left, right in zip(projection, projection[1:]))


def _edge_sort_key(edge: dict[str, Any]) -> tuple[Any, ...]:
    return (
        edge["source"],
        edge["target"],
        edge["relation"],
        edge["direction"],
        edge["inhibitory"],
    )


def _decision_sort_key(row: dict[str, Any]) -> tuple[Any, ...]:
    option_string = (
        (0, "")
        if row["resolved_target_id"] is None
        else (
            1,
            row["resolved_target_id"],
        )
    )
    option_start = (
        (0, 0) if row["source_line_start"] is None else (1, row["source_line_start"])
    )
    option_end = (
        (0, 0) if row["source_line_end"] is None else (1, row["source_line_end"])
    )
    outcome_order = {"RESOLVED": 0, "UNRESOLVED": 1, "AMBIGUOUS": 2}
    return (
        row["source_key"],
        row["source_id"],
        row["target_label"],
        row["relation"],
        outcome_order[row["outcome"]],
        option_string,
        tuple(row["candidate_ids"]),
        option_start,
        option_end,
    )


def _validate_resolution_rows(ownership: dict[str, Any]) -> None:
    for row in ownership["resolution_inputs"]:
        row = _require_exact_fields(row, RESOLUTION_INPUT_FIELDS, "resolution input")
        for field in RESOLUTION_INPUT_FIELDS:
            _require_nonempty_string(row[field], f"resolution input {field}")
        _canonical_relative_posix(row["source_key"], "resolution input source_key")
        if any(row[field] != row[field].strip() for field in RESOLUTION_INPUT_FIELDS):
            raise RunnerError("resolution input is not canonical")
    for row in ownership["resolution_hints"]:
        row = _require_exact_fields(row, RESOLUTION_HINT_FIELDS, "resolution hint")
        for field in RESOLUTION_HINT_FIELDS:
            _require_nonempty_string(row[field], f"resolution hint {field}")
        if any(row[field] != row[field].strip() for field in RESOLUTION_HINT_FIELDS):
            raise RunnerError("resolution hint is not canonical")
    for row in ownership["resolution_decisions"]:
        row = _require_exact_fields(
            row, RESOLUTION_DECISION_FIELDS, "resolution decision"
        )
        for field in ("source_key", "source_id", "target_label", "relation"):
            _require_nonempty_string(row[field], f"resolution decision {field}")
        if any(
            row[field] != row[field].strip()
            for field in ("source_key", "source_id", "target_label", "relation")
        ):
            raise RunnerError("resolution decision is not canonical")
        if row["outcome"] not in {"RESOLVED", "UNRESOLVED", "AMBIGUOUS"}:
            raise RunnerError("resolution decision outcome is invalid")
        if row["resolved_target_id"] is not None:
            _require_nonempty_string(
                row["resolved_target_id"], "resolution decision target"
            )
        if not isinstance(row["candidate_ids"], list) or any(
            not isinstance(candidate, str) or not candidate
            for candidate in row["candidate_ids"]
        ):
            raise RunnerError("resolution decision candidates are malformed")
        for field in ("source_line_start", "source_line_end"):
            if row[field] is not None:
                _require_nonnegative_integer(row[field], f"resolution decision {field}")
        if (row["source_line_start"] is None) is not (
            row["source_line_end"] is None
        ) or (
            row["source_line_start"] is not None
            and row["source_line_start"] > row["source_line_end"]
        ):
            raise RunnerError("resolution decision line range is invalid")
        if not _strictly_increasing(row["candidate_ids"]):
            raise RunnerError("resolution decision candidates are not strictly sorted")

    inputs = ownership["resolution_inputs"]
    hints = ownership["resolution_hints"]
    decisions = ownership["resolution_decisions"]
    if not _strictly_increasing(
        inputs,
        key=lambda row: (
            row["source_key"],
            row["source_id"],
            row["target_label"],
            row["relation"],
        ),
    ):
        raise RunnerError("resolution inputs are not strictly sorted")
    if not _strictly_increasing(
        hints,
        key=lambda row: (row["source_id"], row["target_label"], row["import_path"]),
    ):
        raise RunnerError("resolution hints are not strictly sorted")
    if not _strictly_increasing(decisions, key=_decision_sort_key):
        raise RunnerError("resolution decisions are not strictly sorted")
    decision_inputs = [
        {
            "source_key": row["source_key"],
            "source_id": row["source_id"],
            "target_label": row["target_label"],
            "relation": row["relation"],
        }
        for row in decisions
    ]
    if decision_inputs != inputs:
        raise RunnerError("resolution inputs differ from their decisions")
    input_pairs = {(row["source_id"], row["target_label"]) for row in inputs}
    hint_pairs = [(row["source_id"], row["target_label"]) for row in hints]
    if any(pair not in input_pairs for pair in hint_pairs) or len(hint_pairs) != len(
        set(hint_pairs)
    ):
        raise RunnerError("resolution hints do not bind unique resolution inputs")
    for decision in decisions:
        claims = ownership["claims_by_source"].get(decision["source_key"])
        if (
            not isinstance(claims, dict)
            or decision["source_id"] not in claims["node_ids"]
        ):
            raise RunnerError("resolution decision source is not owned")
        target = decision["resolved_target_id"]
        candidates = decision["candidate_ids"]
        if decision["outcome"] == "UNRESOLVED":
            if target is not None or candidates:
                raise RunnerError("UNRESOLVED decision carries a target")
            continue
        if (
            target is None
            or target not in candidates
            or (decision["outcome"] == "AMBIGUOUS" and len(candidates) < 2)
            or (decision["outcome"] == "RESOLVED" and not candidates)
        ):
            raise RunnerError("resolved decision candidate shape is invalid")
        if not any(
            edge["source"] == decision["source_id"]
            and edge["target"] == target
            and edge["relation"] == decision["relation"]
            and edge["direction"] == 0
            and edge["inhibitory"] is False
            for edge in claims["edges"]
        ):
            raise RunnerError("resolved decision lacks its exact owned edge")


def _validate_ownership_manifest(
    value: Any,
    *,
    spec: OwnerSpec,
    preview: dict[str, Any],
    binary_digest: str,
) -> dict[str, Any]:
    ownership = _require_exact_fields(
        value, OWNERSHIP_MANIFEST_FIELDS, "code ownership manifest"
    )
    expected_source_digests = dict(spec.source_digests)
    if not expected_source_digests:
        raise RunnerError("owner has no sealed source digest binding")
    if (
        ownership["schema"] != CODE_OWNERSHIP_MANIFEST_SCHEMA
        or not _same_root(ownership["root_identity"], spec.root)
        or ownership["exact_source_key"] is not None
        or ownership["base_ownership_digest"] is not None
        or ownership["source_digests"] != expected_source_digests
        or not isinstance(ownership["claims_by_source"], dict)
        or set(ownership["claims_by_source"]) != set(expected_source_digests)
        or ownership["source_projection_digest"]
        != preview["candidate_source_projection_digest"]
        or ownership["pipeline_digest"] != preview["candidate_pipeline_digest"]
        or ownership["ownership_digest"] != preview["candidate_ownership_digest"]
    ):
        raise RunnerError(
            "ownership manifest does not bind the exact live source candidate"
        )
    for source, claims_value in ownership["claims_by_source"].items():
        claims = _require_exact_fields(
            claims_value, SOURCE_CLAIMS_FIELDS, f"source claims for {source}"
        )
        if claims["source_hint"] != source:
            raise RunnerError("source claim hint differs from its source key")
        if not isinstance(claims["node_ids"], list) or any(
            not _valid_external_id(node_id) for node_id in claims["node_ids"]
        ):
            raise RunnerError("source claim node identities are malformed")
        if not _strictly_increasing(claims["node_ids"]):
            raise RunnerError("source claim node identities are not strictly sorted")
        if not isinstance(claims["edges"], list):
            raise RunnerError("source claim edges are malformed")
        for edge in claims["edges"]:
            _validate_claimed_edge(edge, f"source claim edge for {source}")
        if not _strictly_increasing(claims["edges"], key=_edge_sort_key):
            raise RunnerError("source claim edges are not strictly sorted")
    if (
        ownership["graph_finalized"] is not True
        or ownership["pending_edge_count"] != 0
        or ownership["bidirectional_mirrors_valid"] is not True
        or ownership["csr_shape_valid"] is not True
        or ownership["reverse_csr_valid"] is not True
        or ownership["coverage"] != "COMPLETE"
    ):
        raise RunnerError("ownership manifest is not a finalized COMPLETE graph")
    for field in (
        "orphan_node_slots",
        "multiply_identified_node_slots",
        "invalid_identity_ids",
        "out_of_range_identity_ids",
        "orphan_edge_slots",
        "unowned_nodes",
        "unowned_edges",
        "dangling_node_claims",
        "dangling_edge_claims",
        "duplicate_graph_edges",
    ):
        if ownership[field] != []:
            raise RunnerError(f"ownership manifest reports non-empty {field}")
    for field in (
        "resolution_inputs",
        "resolution_hints",
        "resolution_decisions",
    ):
        if not isinstance(ownership[field], list):
            raise RunnerError(f"ownership manifest {field} is malformed")
    _validate_resolution_rows(ownership)
    for field in (
        "resolution_input_digest",
        "resolution_hint_digest",
        "resolution_digest",
        "lineage_digest",
    ):
        _require_digest(ownership[field], f"ownership {field}", prefixed=False)
    pipeline = _validate_pipeline_receipt(
        ownership["pipeline_receipt"],
        source_count=len(expected_source_digests),
        source_paths=tuple(expected_source_digests),
        binary_digest=binary_digest,
        candidate_pipeline_digest=preview["candidate_pipeline_digest"],
        manifest_pipeline_digest=ownership["pipeline_digest"],
    )
    recomputed = _recomputed_ownership_digests(ownership)
    for field_name, expected_digest in recomputed.items():
        if ownership[field_name] != expected_digest:
            raise RunnerError(
                f"ownership {field_name} does not bind its exact typed content"
            )
    if recomputed["ownership_digest"] != preview["candidate_ownership_digest"]:
        raise RunnerError("preview ownership digest does not bind the exact manifest")
    return {
        "ownership_manifest_exact": True,
        "source_digests_exact": True,
        "source_digest_count": len(expected_source_digests),
        **pipeline,
    }


def _validate_ingest_output(value: Any, *, source_count: int) -> dict[str, Any]:
    output = _require_exact_fields(value, INGEST_OUTPUT_FIELDS, "graph ingest output")
    if (
        output["mode"] != "replace"
        or output["adapter"] != "code"
        or output["namespace"] is not None
        or output["files_scanned"] != source_count
        or output["files_parsed"] != source_count
        or output["light_evidence_resolved"] != 0
        or output["light_evidence_unresolved"] != 0
    ):
        raise RunnerError(
            "graph ingest output does not cover the exact clean code root"
        )
    for field in (
        "files_scanned",
        "files_parsed",
        "nodes_created",
        "edges_created",
        "node_count",
        "edge_count",
        "light_evidence_resolved",
        "light_evidence_unresolved",
    ):
        _require_nonnegative_integer(output[field], f"graph ingest {field}")
    elapsed = output["elapsed_ms"]
    if (
        not isinstance(elapsed, (int, float))
        or isinstance(elapsed, bool)
        or elapsed < 0
        or not math.isfinite(elapsed)
    ):
        raise RunnerError("graph ingest elapsed_ms is invalid")
    freshness = _require_exact_fields(
        output["memory_freshness"],
        frozenset({"stale_evidence_count", "stale_evidence"}),
        "graph ingest memory freshness",
    )
    if freshness != {"stale_evidence_count": 0, "stale_evidence": []}:
        raise RunnerError("fresh isolated runtime reported stale source evidence")
    return {"ingest_output_exact": True, "files_scanned": source_count}


def _validate_checkpoint_ack(
    value: Any, *, preview: dict[str, Any], graph_generation_after: int
) -> dict[str, Any]:
    ack = _require_exact_fields(value, CHECKPOINT_ACK_FIELDS, "graph checkpoint ACK")
    if (
        ack["schema"] != CHECKPOINT_ACK_SCHEMA
        or ack["brain_id"] != preview["actor_brain_id"]
        or not isinstance(ack["checkpoint_id"], str)
        or not ack["checkpoint_id"].strip()
        or _require_nonnegative_integer(ack["epoch"], "checkpoint epoch") == 0
        or _require_nonnegative_integer(ack["revision"], "checkpoint revision") == 0
        or _require_nonnegative_integer(ack["confirmed_at_unix_ms"], "checkpoint time")
        == 0
        or _require_nonnegative_integer(ack["generation"], "checkpoint generation")
        < graph_generation_after
    ):
        raise RunnerError("checkpoint ACK does not bind the installed actor generation")
    _require_digest(
        ack["current_pointer_digest"], "checkpoint current pointer", prefixed=False
    )
    return {
        "checkpoint_ack_exact": True,
        "checkpoint_id": ack["checkpoint_id"],
        "checkpoint_generation": ack["generation"],
        "current_pointer_digest": ack["current_pointer_digest"],
    }


def _validate_external_mutation_response(
    response: dict[str, Any],
    execute_request: dict[str, Any],
    preview: dict[str, Any],
    lease: str,
    spec: OwnerSpec,
    binary_digest: str,
) -> dict[str, Any]:
    response = _require_exact_fields(
        response,
        frozenset(
            {
                "schema",
                "request_id",
                "semantic_action",
                "semantic_payload_digest",
                "operation_object_digest",
                "authorization_lease_id",
                "authorization_reservation_id",
                "journal_operation_id",
                "outcome_digest",
                "graph_resync_required",
                "reconciliation_state",
                "result",
            }
        ),
        "external mutation response",
    )
    if (
        response["schema"] != EXTERNAL_MUTATION_RESPONSE_SCHEMA
        or response["request_id"] != execute_request["request_id"]
        or response["semantic_action"] != preview["semantic_action"]
        or response["semantic_payload_digest"] != preview["semantic_payload_digest"]
        or response["operation_object_digest"] != preview["operation_object_digest"]
        or response["authorization_lease_id"] != lease
    ):
        raise RunnerError(
            "external mutation response has a foreign digest or lease binding"
        )
    for field in ("authorization_reservation_id", "journal_operation_id"):
        _require_nonempty_string(response[field], f"external mutation {field}")
    _require_digest(
        response["outcome_digest"], "external mutation outcome", prefixed=False
    )
    if response["graph_resync_required"] is not False:
        raise RunnerError("graph ingest unexpectedly requires an external resync")
    if response["reconciliation_state"] != "RECONCILED":
        raise RunnerError("graph ingest did not reach RECONCILED")
    result = _require_exact_fields(
        response["result"], GRAPH_INGEST_RESULT_FIELDS, "graph ingest result"
    )
    if (
        result["mode"] != "REPLACE"
        or not _same_root(result["root_identity"], spec.root)
        or result["reconciliation_brain_id"] != preview["actor_brain_id"]
        or result["parent"] is not None
        or result["actor_checkpoint_required"] is not True
        or result["candidate_ownership_digest"] != preview["candidate_ownership_digest"]
        or result["candidate_source_projection_digest"]
        != preview["candidate_source_projection_digest"]
        or result["candidate_pipeline_digest"] != preview["candidate_pipeline_digest"]
    ):
        raise RunnerError(
            "graph ingest result has a foreign owner or source projection"
        )
    generation_before = _require_nonnegative_integer(
        result["graph_generation_before"], "graph generation before"
    )
    generation_after = _require_nonnegative_integer(
        result["graph_generation_after"], "graph generation after"
    )
    if (
        generation_before != preview["expected_graph_generation"]
        or generation_after <= generation_before
    ):
        raise RunnerError("graph ingest generations do not prove an installed mutation")
    ownership = _validate_ownership_manifest(
        result["ownership_manifest"],
        spec=spec,
        preview=preview,
        binary_digest=binary_digest,
    )
    recomputed_outcome = _rust_domain_digest(
        GRAPH_INGEST_OUTCOME_DIGEST_DOMAIN,
        [
            response["operation_object_digest"],
            result["mode"],
            result["root_identity"],
            result["ownership_manifest"]["ownership_digest"],
            result["ownership_manifest"]["source_projection_digest"],
            result["parent"],
        ],
    )
    if response["outcome_digest"] != recomputed_outcome:
        raise RunnerError("external mutation outcome digest does not bind the result")
    ingest = _validate_ingest_output(
        result["ingest_output"], source_count=len(spec.source_digests)
    )
    checkpoint = _validate_checkpoint_ack(
        result["checkpoint_ack"],
        preview=preview,
        graph_generation_after=generation_after,
    )
    return {
        **ownership,
        **ingest,
        **checkpoint,
        "graph_generation_before": generation_before,
        "graph_generation_after": generation_after,
    }


def execute_governed_graph_ingest(
    spec: OwnerSpec,
    client: McpHttpClient,
    provider: AuthorityProvider,
    assembly: AuthorityAssembly,
    verifier: AuthorizationReceiptVerifier | None,
    *,
    diagnostic: bool,
    lane: str,
    binary_digest: str,
) -> dict[str, Any]:
    """Execute the three-tool protocol on one unchanged MCP client/session."""
    if not client.session_id:
        raise RunnerError("governed ingest requires an initialized MCP session")
    session_id = client.session_id
    preview_request_id = f"g6-preview-{lane}-{spec.owner_id}"
    started = time.perf_counter_ns()
    preview = client.call_tool(
        "graph_ingest_preview",
        {
            "schema": GRAPH_PREVIEW_REQUEST_SCHEMA,
            "request_id": preview_request_id,
            "mode": "REPLACE",
            "include_dotfiles": False,
            "dotfile_patterns": [],
            "parent": None,
        },
    )
    if client.session_id != session_id:
        raise RunnerError("MCP session changed during graph-ingest preview")
    _validate_graph_preview(spec, session_id, preview_request_id, preview)

    provider_request = _authority_provider_request(
        spec,
        preview,
        assembly,
        diagnostic=diagnostic,
        lane=lane,
        binary_digest=binary_digest,
    )
    provider_response = provider.authorize(provider_request)
    authorization_request = _validate_provider_authorization(
        provider_response,
        provider_request,
        spec,
        preview,
        assembly,
        binary_digest,
    )
    authorization_response = client.call_tool(
        "authority_authorize", authorization_request
    )
    if client.session_id != session_id:
        raise RunnerError("MCP session changed during authority authorization")
    lease, authority_proof = _validate_authorization_response(
        authorization_response,
        authorization_request,
        preview,
        assembly,
        verifier,
        diagnostic=diagnostic,
    )

    client.bind_authorization_lease(lease)
    try:
        mutation_response = client.call_tool(
            "external_mutation_service", preview["execute_request"]
        )
    finally:
        client.clear_authorization_lease()
    if client.session_id != session_id:
        raise RunnerError("MCP session changed during external mutation")
    mutation_proof = _validate_external_mutation_response(
        mutation_response,
        preview["execute_request"],
        preview,
        lease,
        spec,
        binary_digest,
    )
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000
    result = mutation_response["result"]
    ingest_output = result.get("ingest_output")
    return {
        "repo_id": spec.repo_id,
        "owner_id": spec.owner_id,
        "source_revision": spec.source_revision,
        "file_set_digest": spec.file_set_digest,
        "semantic_payload_digest": preview["semantic_payload_digest"],
        "operation_object_digest": preview["operation_object_digest"],
        "mcp_session_id": session_id,
        "candidate_ownership_digest": preview["candidate_ownership_digest"],
        "candidate_source_projection_digest": preview[
            "candidate_source_projection_digest"
        ],
        "candidate_pipeline_digest": preview["candidate_pipeline_digest"],
        "authorization_lease_bound": True,
        "authority_receipt": authority_proof,
        "production_authority_receipt_proven": authority_proof[
            "production_authority_receipt_proven"
        ],
        "reconciliation_state": mutation_response["reconciliation_state"],
        "files_scanned": ingest_output.get("files_scanned")
        if isinstance(ingest_output, dict)
        else None,
        "files_parsed": ingest_output.get("files_parsed")
        if isinstance(ingest_output, dict)
        else None,
        "node_count": ingest_output.get("node_count")
        if isinstance(ingest_output, dict)
        else None,
        "edge_count": ingest_output.get("edge_count")
        if isinstance(ingest_output, dict)
        else None,
        "mutation_proof": mutation_proof,
        "governed_ingest_latency_ms": round(elapsed_ms, 6),
    }


def authority_run_metadata(
    *, diagnostic: bool, assembly: AuthorityAssembly
) -> dict[str, Any]:
    return {
        "authority_mode": "diagnostic_software_permitted" if diagnostic else "formal",
        "authority_provider_kind": assembly.provider_kind,
        "authority_provider_claimed_production_assembly": (
            assembly.production_authority_assembly
        ),
        "production_authority_assembly_proven": False,
        "authority_assembly_id": assembly.assembly_id,
        "authority_assembly_digest": assembly.assembly_digest,
        "authority_assembly_digest_verified": assembly.expected_digest_verified,
        "authority_provider_executable_digest": assembly.provider_executable_digest,
        "authority_owner_security_config_digest": assembly.owner_security_config_digest,
        "authority_key_registry_epoch": assembly.key_registry_epoch,
        "authority_receipt_key_id": assembly.verification_key.key_id,
        "authority_blind_boundary_kind": assembly.blind_boundary_kind,
        "authority_blind_boundary_proven": assembly.blind_boundary_proven,
    }


def _rust_domain_digest(domain: str, value: Any) -> str:
    payload = _canonical_bytes(value)
    prefix = b"m1nd-domain-separated-sha256-v1\0"
    framed = (
        prefix
        + struct.pack(">Q", len(domain.encode("utf-8")))
        + domain.encode("utf-8")
        + struct.pack(">Q", len(payload))
        + payload
    )
    return hashlib.sha256(framed).hexdigest()


def validate_calibration_receipt(receipt: Any) -> dict[str, Any]:
    """Validate the exact Rust wire receipt and return a scorer-safe projection."""
    receipt = _require_exact_fields(
        receipt,
        frozenset(
            {
                "schema",
                "status",
                "signal",
                "receipt_digest",
                "tau",
                "sample_size",
                "measured_precision",
                "coverage",
                "target_alpha",
                "calibrated_at_ms",
            }
        ),
        "seek calibration receipt",
    )
    if (
        receipt["schema"] != SEEK_CALIBRATION_RECEIPT_SCHEMA
        or receipt["status"] != SEEK_CALIBRATION_RECEIPT_STATUS
        or receipt["signal"] != SEEK_CALIBRATION_SIGNAL
    ):
        raise RunnerError("seek calibration receipt identity mismatch")
    raw_digest = receipt["receipt_digest"]
    if (
        not isinstance(raw_digest, str)
        or re.fullmatch(r"[0-9a-f]{64}", raw_digest) is None
    ):
        raise RunnerError(
            "seek calibration receipt digest is not raw lowercase SHA-256"
        )
    for field in ("tau", "measured_precision", "coverage", "target_alpha"):
        value = receipt[field]
        if (
            not isinstance(value, (int, float))
            or isinstance(value, bool)
            or not math.isfinite(float(value))
            or not 0.0 <= float(value) <= 1.0
        ):
            raise RunnerError(f"seek calibration receipt {field} is invalid")
    if (
        not isinstance(receipt["sample_size"], int)
        or isinstance(receipt["sample_size"], bool)
        or receipt["sample_size"] <= 0
        or not isinstance(receipt["calibrated_at_ms"], int)
        or isinstance(receipt["calibrated_at_ms"], bool)
        or receipt["calibrated_at_ms"] <= 0
    ):
        raise RunnerError("seek calibration receipt sample/time evidence is invalid")
    digest_projection = {
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
    }
    if (
        _rust_domain_digest(SEEK_CALIBRATION_DIGEST_DOMAIN, digest_projection)
        != raw_digest
    ):
        raise RunnerError(
            "seek calibration receipt digest does not bind its complete row"
        )
    return {
        "receipt_digest": f"sha256:{raw_digest}",
        "receipt_schema": receipt["schema"],
        "signal": receipt["signal"],
        "tau": float(receipt["tau"]),
        "sample_size": receipt["sample_size"],
        "measured_precision": float(receipt["measured_precision"]),
        "coverage": float(receipt["coverage"]),
        "target_alpha": float(receipt["target_alpha"]),
        "calibrated_at_ms": receipt["calibrated_at_ms"],
    }


def validate_metric_spec_for_runner(spec: Any) -> dict[str, Any]:
    if not isinstance(spec, dict):
        raise RunnerError("metric spec is not an object")
    if spec.get("schema") != METRIC_SPEC_SCHEMA or spec.get("version") != 2:
        raise RunnerError("metric spec is not the ratified v2 schema")
    if spec.get("self_digest") != _self_digest(spec):
        raise RunnerError("metric spec self_digest mismatch")
    calibration = spec.get("calibration")
    if not isinstance(calibration, dict):
        raise RunnerError("metric spec calibration gate is absent")
    for field in ("minimum_calibration_sample_size", "minimum_authorized_action_count"):
        value = calibration.get(field)
        if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
            raise RunnerError(f"metric spec calibration field {field} is invalid")
    for field in (
        "minimum_calibration_precision",
        "minimum_calibration_coverage",
        "minimum_calibrated_task_fraction",
    ):
        value = calibration.get(field)
        if (
            not isinstance(value, (int, float))
            or isinstance(value, bool)
            or not 0.0 < float(value) <= 1.0
        ):
            raise RunnerError(f"metric spec calibration field {field} is invalid")
    return calibration


def build_calibration_summary(
    receipts: list[dict[str, Any] | None],
    measurements: list[dict[str, Any]],
    calibration_spec: dict[str, Any],
) -> dict[str, Any]:
    present = [receipt for receipt in receipts if receipt is not None]
    unique_digests = {receipt["receipt_digest"] for receipt in present}
    first = present[0] if present and len(unique_digests) == 1 else None
    calibrated_count = len(present)
    authorized_action_count = sum(
        row.get("verdict") == "act" for row in measurements if isinstance(row, dict)
    )
    task_count = len(measurements)
    calibrated_fraction = calibrated_count / task_count if task_count else 0.0
    armed = bool(
        first is not None
        and calibrated_count == task_count
        and first["sample_size"] >= calibration_spec["minimum_calibration_sample_size"]
        and first["measured_precision"]
        >= calibration_spec["minimum_calibration_precision"]
        and first["coverage"] >= calibration_spec["minimum_calibration_coverage"]
        and calibrated_fraction >= calibration_spec["minimum_calibrated_task_fraction"]
        and authorized_action_count
        >= calibration_spec["minimum_authorized_action_count"]
    )
    return {
        "schema": CALIBRATION_RUN_SCHEMA,
        "status": "armed" if armed else PROOF_NOT_PROVEN,
        "receipt_digest": first["receipt_digest"] if first else None,
        "receipt_schema": first["receipt_schema"] if first else None,
        "signal": first["signal"] if first else None,
        "tau": first["tau"] if first else None,
        "sample_size": first["sample_size"] if first else None,
        "measured_precision": first["measured_precision"] if first else None,
        "coverage": first["coverage"] if first else None,
        "target_alpha": first["target_alpha"] if first else None,
        "calibrated_at_ms": first["calibrated_at_ms"] if first else None,
        "calibrated_task_count": calibrated_count,
        "authorized_action_count": authorized_action_count,
    }


def extract_measurement(
    task_id: str,
    north_latency_ms: float,
    seek_latency_ms: float,
    seek_result: dict[str, Any],
) -> tuple[dict[str, Any], str | None, dict[str, Any] | None]:
    anchors: list[str] = []
    seen: set[str] = set()
    for result in seek_result.get("results", []):
        node_id = result.get("node_id") if isinstance(result, dict) else None
        if isinstance(node_id, str) and node_id and node_id not in seen:
            anchors.append(node_id)
            seen.add(node_id)
        if len(anchors) == 5:
            break

    trust_envelope = seek_result.get("trust_envelope")
    if not isinstance(trust_envelope, dict):
        raise RunnerError("seek result lacks a trust envelope")
    raw_verdict = trust_envelope.get("verdict")
    if raw_verdict in VALID_VERDICTS:
        verdict = raw_verdict
    elif not anchors or raw_verdict == "abstain":
        verdict = "abstain"
    else:
        verdict = "reverify"

    raw_results = seek_result.get("results", [])
    ranked_scores = [
        round(float(result["score"]), 8)
        for result in raw_results[:5]
        if isinstance(result, dict)
        and isinstance(result.get("score"), (int, float))
        and math.isfinite(float(result["score"]))
    ]
    sufficiency = seek_result.get("sufficiency", {})
    wire_receipt = trust_envelope.get("calibration_receipt")
    calibration = (
        validate_calibration_receipt(wire_receipt) if wire_receipt is not None else None
    )
    if trust_envelope.get("calibrated") is not (calibration is not None):
        raise RunnerError("seek calibrated flag and calibration receipt disagree")
    if raw_verdict == "act" and calibration is None:
        raise RunnerError("runtime emitted act without a valid calibration receipt")

    return (
        {
            "task_id": task_id,
            "ranked_anchor_ids": anchors,
            "verdict": verdict,
            "north_latency_ms": round(north_latency_ms, 6),
            "seek_latency_ms": round(seek_latency_ms, 6),
            "north_executed": True,
            "seek_executed": True,
            "ranked_scores": ranked_scores,
            "relevance_clearing_total": seek_result.get("relevance_clearing_total"),
            "sufficiency": {
                "state": sufficiency.get("state"),
                "captured": sufficiency.get("captured"),
                "marginal_score": sufficiency.get("marginal_score"),
                "top_score": sufficiency.get("top_score"),
            },
            "trust_envelope": {
                "calibrated": calibration is not None,
                "score": trust_envelope.get("score"),
                "verdict": verdict,
                "calibration_receipt_digest": (
                    calibration["receipt_digest"] if calibration is not None else None
                ),
            },
        },
        raw_verdict if isinstance(raw_verdict, str) else None,
        calibration,
    )


def _validate_run_proof_metadata(
    metadata: dict[str, Any], binary_digest: Any
) -> tuple[list[str], bool]:
    errors: list[str] = []
    formal = metadata.get("formal_preflights")
    if not isinstance(formal, dict) or set(formal) != FORMAL_PREFLIGHT_FIELDS:
        return ["formal_preflights violates its closed JSON field set"], False
    missing = formal["missing"]
    if not isinstance(missing, list) or any(
        not isinstance(item, str) or not item for item in missing
    ):
        errors.append("formal_preflights missing list is malformed")
        missing = []
    blind = formal["authority_blind_boundary"]
    if (
        not isinstance(blind, dict)
        or set(blind) != {"kind", "proven"}
        or blind.get("kind") != metadata.get("authority_blind_boundary_kind")
        or blind.get("proven") is not metadata.get("authority_blind_boundary_proven")
    ):
        errors.append("formal authority blind-boundary proof is incoherent")
        blind = {}
    path_topology = formal["path_topology"]
    if (
        not isinstance(path_topology, dict)
        or set(path_topology) != PATH_TOPOLOGY_PROOF_FIELDS
        or not isinstance(path_topology.get("paths"), dict)
    ):
        errors.append("formal path topology proof is malformed")
        path_topology = {}

    for field_name, prefixed in (
        ("authority_assembly_digest", False),
        ("authority_provider_executable_digest", True),
        ("authority_owner_security_config_digest", True),
    ):
        value = metadata.get(field_name)
        pattern = r"sha256:[0-9a-f]{64}" if prefixed else r"[0-9a-f]{64}"
        if not isinstance(value, str) or re.fullmatch(pattern, value) is None:
            errors.append(f"run_metadata invalid {field_name}")
    if (
        not isinstance(metadata.get("authority_key_registry_epoch"), int)
        or isinstance(metadata.get("authority_key_registry_epoch"), bool)
        or metadata["authority_key_registry_epoch"] <= 0
    ):
        errors.append("run_metadata authority key registry epoch is invalid")

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

    topology_repos: list[Any] = []
    topology_bindings_proven = bool(topologies)
    for topology in topologies:
        if not isinstance(topology, dict) or set(topology) != OWNER_TOPOLOGY_FIELDS:
            errors.append("owner topology violates its closed JSON field set")
            topology_bindings_proven = False
            continue
        topology_repos.append(topology["repo_id"])
        readiness = topology["readiness"]
        cleanup = topology["cleanup"]
        if not isinstance(readiness, dict) or set(readiness) != OWNER_READINESS_FIELDS:
            errors.append(f"{topology['repo_id']}: owner readiness proof is malformed")
            topology_bindings_proven = False
            continue
        for digest_field in ("registry_entry_digest", "manifest_digest"):
            value = readiness[digest_field]
            if (
                not isinstance(value, str)
                or re.fullmatch(r"(?:sha256:)?[0-9a-f]{64}", value) is None
            ):
                errors.append(
                    f"{topology['repo_id']}: invalid readiness {digest_field}"
                )
                topology_bindings_proven = False
        binding = bool(
            topology["process_isolated"] is True
            and topology["mcp_session_isolated"] is True
            and readiness["owner_binding_proven"] is True
            and readiness["token_captured_once"] is True
            and readiness["binary_digest"] == binary_digest
            and isinstance(topology["port"], int)
            and not isinstance(topology["port"], bool)
            and 1 <= topology["port"] <= 65_535
            and topology["port"] != INSTALLED_OWNER_PORT
            and isinstance(cleanup, dict)
        )
        topology_bindings_proven = topology_bindings_proven and binding
        if not binding:
            errors.append(
                f"{topology['repo_id']}: owner readiness binding is incomplete"
            )

    cleanup_repos: list[Any] = []
    cleanup_proven = bool(cleanups)
    for cleanup in cleanups:
        if not isinstance(cleanup, dict):
            errors.append("owner cleanup proof is malformed")
            cleanup_proven = False
            continue
        cleanup_repos.append(cleanup.get("repo_id"))
        proven = bool(
            cleanup.get("same_session_for_owner_lifetime") is True
            and cleanup.get("session_delete_proven") is True
            and cleanup.get("process_group_terminated") is True
            and cleanup.get("cleanup_complete") is True
        )
        cleanup_proven = cleanup_proven and proven
        if not proven:
            errors.append(
                f"{cleanup.get('repo_id', '<missing>')}: cleanup is incomplete"
            )

    ingest_repos: list[Any] = []
    authority_receipts_proven = bool(ingests)
    for ingest in ingests:
        if not isinstance(ingest, dict) or set(ingest) != GOVERNED_INGEST_FIELDS:
            errors.append("governed ingest violates its closed JSON field set")
            authority_receipts_proven = False
            continue
        ingest_repos.append(ingest["repo_id"])
        receipt = ingest["authority_receipt"]
        if (
            not isinstance(receipt, dict)
            or set(receipt) != AUTHORITY_RECEIPT_PROOF_FIELDS
        ):
            errors.append(f"{ingest['repo_id']}: authority receipt proof is malformed")
            authority_receipts_proven = False
            continue
        proven = bool(
            ingest["authorization_lease_bound"] is True
            and ingest["production_authority_receipt_proven"] is True
            and ingest["reconciliation_state"] == "RECONCILED"
            and receipt["production_authority_receipt_proven"] is True
            and receipt["control_verified_ed25519"] is True
            and receipt["receipt_core_digest_verified"] is True
            and receipt["assembly_digest_verified"] is True
            and receipt["signature_verified"] is True
            and receipt["clock_verified"] is True
            and receipt["key_lifecycle_verified"] is True
            and receipt["receipt_signer_metadata_production"] is True
            and receipt["key_registry_epoch"]
            == metadata.get("authority_key_registry_epoch")
            and receipt["key_id"] == metadata.get("authority_receipt_key_id")
            and receipt["algorithm"] == "ED25519"
        )
        authority_receipts_proven = authority_receipts_proven and proven
        if (
            ingest["production_authority_receipt_proven"] is True
            or receipt["production_authority_receipt_proven"] is True
        ) and not proven:
            errors.append(
                f"{ingest['repo_id']}: production authority proof is incomplete"
            )

    same_repo_set = bool(topology_repos) and (
        len(topology_repos) == len(set(topology_repos))
        and set(topology_repos) == set(cleanup_repos) == set(ingest_repos)
    )
    if not same_repo_set:
        errors.append("owner topology/cleanup/ingest repository sets differ")
    if metadata.get("governed_setup_mutations_executed") != len(ingests):
        errors.append("governed setup mutation count mismatch")
    if formal["authority_receipts_proven"] is not authority_receipts_proven:
        errors.append("formal authority receipt summary is not independently derived")
    if (
        metadata.get("production_authority_assembly_proven")
        is not authority_receipts_proven
    ):
        errors.append("authority assembly proof summary differs from receipt evidence")

    path_proven = bool(
        path_topology.get("absolute") is True
        and path_topology.get("fresh_mutable_roots") is True
        and path_topology.get("disjoint") is True
        and path_topology.get("symlink_free_path_components") is True
    )
    source_verification = metadata.get("source_verification")
    post_source_verification = metadata.get("post_ingest_source_verification")
    source_live = bool(
        isinstance(source_verification, dict)
        and source_verification.get("exact_live_file_set") is True
    )
    source_post = bool(source_live and post_source_verification == source_verification)
    if (
        formal["source_live_identity"] is not source_live
        or formal["source_post_ingest_identity"] is not source_post
    ):
        errors.append("formal source identity summary is not independently derived")
    derived_complete = bool(
        metadata.get("authority_mode") == "formal"
        and metadata.get("authority_provider_kind") == "production"
        and metadata.get("authority_provider_claimed_production_assembly") is True
        and metadata.get("authority_assembly_digest_verified") is True
        and metadata.get("authority_blind_boundary_proven") is True
        and metadata.get("labels_read") is False
        and formal["delivery"] == "delivery-2-hardened-runner"
        and formal["same_session_readiness_ingest_measurement_delete"] is cleanup_proven
        and formal["process_group_cleanup"] is cleanup_proven
        and formal["source_live_identity"] is source_live
        and formal["source_post_ingest_identity"] is source_post
        and formal["owner_readiness_bindings_proven"] is topology_bindings_proven
        and formal["authority_receipts_proven"] is True
        and blind.get("proven") is True
        and path_proven
        and same_repo_set
        and topology_bindings_proven
        and cleanup_proven
        and authority_receipts_proven
    )
    if (
        formal["complete"] is not derived_complete
        or formal["status"] != ("PROVEN" if derived_complete else PROOF_NOT_PROVEN)
        or (derived_complete and missing)
        or (not derived_complete and not missing)
    ):
        errors.append("formal preflight summary is not independently derived")
    return errors, derived_complete


def validate_unscored_artifact(
    artifact: dict[str, Any], queries: dict[str, Any]
) -> list[str]:
    errors: list[str] = []
    if not isinstance(artifact, dict) or set(artifact) != RESULT_FIELDS:
        errors.append("result violates its closed JSON field set")
        if not isinstance(artifact, dict):
            return errors
    if artifact.get("schema") != RESULT_SCHEMA:
        errors.append("unsupported result schema")
    if artifact.get("self_digest") != _self_digest(artifact):
        errors.append("result self_digest mismatch")
    manifest = queries.get("source_manifest")
    if not isinstance(manifest, dict):
        manifest = {}
    expected_bindings = {
        "corpus_id": queries.get("corpus_id"),
        "corpus_digest": queries.get("corpus_digest"),
        "public_corpus_self_digest": queries.get("self_digest"),
        "source_manifest_digest": manifest.get("manifest_digest"),
        "source_revision": manifest.get("source_commit"),
    }
    for field, expected in expected_bindings.items():
        if not isinstance(expected, str) or artifact.get(field) != expected:
            errors.append(f"{field} mismatch")
    if artifact.get("lane") not in {"current", "baseline"}:
        errors.append("invalid result lane")
    for field in ("run_id", "system_revision"):
        if not isinstance(artifact.get(field), str) or not artifact[field].strip():
            errors.append(f"{field} absent")
    for field in (
        "corpus_digest",
        "public_corpus_self_digest",
        "sealed_corpus_self_digest",
        "source_manifest_digest",
        "binary_digest",
        "runner_digest",
        "metric_spec_digest",
    ):
        value = artifact.get(field)
        if (
            not isinstance(value, str)
            or re.fullmatch(r"sha256:[0-9a-f]{64}", value) is None
        ):
            errors.append(f"invalid {field}")

    metadata = artifact.get("run_metadata")
    if not isinstance(metadata, dict) or set(metadata) != RUN_METADATA_FIELDS:
        errors.append("run_metadata violates its closed JSON field set")
        metadata = metadata if isinstance(metadata, dict) else {}
    score_eligible = metadata.get("score_eligible")
    if (
        metadata.get("schema") != RUN_METADATA_SCHEMA
        or metadata.get("lane") != artifact.get("lane")
        or metadata.get("run_id") != artifact.get("run_id")
        or metadata.get("transport") != "mcp-http-loopback"
        or metadata.get("unscored") is not True
        or not isinstance(score_eligible, bool)
        or metadata.get("diagnostic_only") is not (not score_eligible)
        or metadata.get("proof_state")
        != ("PROVEN" if score_eligible else PROOF_NOT_PROVEN)
    ):
        errors.append("run_metadata identity/proof coherence mismatch")
    for field in ("generated_at", "started_at"):
        if not isinstance(metadata.get(field), str) or not metadata[field].strip():
            errors.append(f"run_metadata {field} absent")
    expected_tasks = queries.get("tasks")
    if not isinstance(expected_tasks, list):
        expected_tasks = []
    if metadata.get("task_count") != len(expected_tasks):
        errors.append("run_metadata task_count mismatch")
    if metadata.get("actions_executed") != 0:
        errors.append("benchmark actions_executed must remain zero")
    if metadata.get("benchmark_task_actions_executed") != 0:
        errors.append("benchmark task actions must remain zero")
    metadata_errors = metadata.get("errors")
    if not isinstance(metadata_errors, list) or any(
        not isinstance(item, str) for item in metadata_errors
    ):
        errors.append("run_metadata errors are malformed")
        metadata_errors = []
    calibration = metadata.get("calibration")
    proof_errors, formal_complete = _validate_run_proof_metadata(
        metadata, artifact.get("binary_digest")
    )
    errors.extend(proof_errors)
    if score_eligible and (
        not formal_complete
        or not isinstance(calibration, dict)
        or calibration.get("status") != "armed"
        or metadata_errors
        or metadata.get("source_verification")
        != metadata.get("post_ingest_source_verification")
    ):
        errors.append("score eligibility lacks complete formal proof")

    measurements = artifact.get("measurements")
    if not isinstance(measurements, list):
        return errors + ["measurements absent"]
    expected = [
        task.get("task_id") for task in expected_tasks if isinstance(task, dict)
    ]
    observed = [row.get("task_id") for row in measurements if isinstance(row, dict)]
    if len(observed) != len(set(observed)):
        errors.append("duplicate task_id")
    if set(observed) != set(expected) or len(observed) != len(expected):
        errors.append(
            f"task coverage mismatch (expected={len(expected)}, observed={len(observed)})"
        )
    for row in measurements:
        if not isinstance(row, dict):
            errors.append("non-object measurement")
            continue
        if set(row) != MEASUREMENT_FIELDS:
            errors.append(
                f"{row.get('task_id', '<missing>')}: closed field set mismatch"
            )
        task_id = row.get("task_id", "<missing>")
        anchors = row.get("ranked_anchor_ids")
        if (
            not isinstance(anchors, list)
            or len(anchors) > 5
            or len(anchors) != len(set(anchors))
            or any(not isinstance(anchor, str) or not anchor for anchor in anchors)
        ):
            errors.append(f"{task_id}: invalid ranked anchors")
        if row.get("verdict") not in VALID_VERDICTS:
            errors.append(f"{task_id}: invalid verdict")
        if row.get("north_executed") is not True:
            errors.append(f"{task_id}: north call was not executed")
        if row.get("seek_executed") is not True:
            errors.append(f"{task_id}: seek call was not executed")
        for field in ("north_latency_ms", "seek_latency_ms"):
            value = row.get(field)
            if (
                not isinstance(value, (int, float))
                or isinstance(value, bool)
                or value < 0
                or not math.isfinite(value)
            ):
                errors.append(f"{task_id}: invalid {field}")
        ranked_scores = row.get("ranked_scores")
        if (
            not isinstance(ranked_scores, list)
            or len(ranked_scores) > 5
            or any(
                not isinstance(score, (int, float))
                or isinstance(score, bool)
                or not math.isfinite(float(score))
                for score in ranked_scores
            )
        ):
            errors.append(f"{task_id}: invalid ranked scores")
        relevance = row.get("relevance_clearing_total")
        if relevance is not None and (
            not isinstance(relevance, (int, float))
            or isinstance(relevance, bool)
            or not math.isfinite(float(relevance))
        ):
            errors.append(f"{task_id}: invalid relevance clearing total")
        sufficiency = row.get("sufficiency")
        if (
            not isinstance(sufficiency, dict)
            or set(sufficiency) != MEASUREMENT_SUFFICIENCY_FIELDS
        ):
            errors.append(f"{task_id}: invalid sufficiency projection")
        else:
            for field in ("marginal_score", "top_score"):
                value = sufficiency[field]
                if value is not None and (
                    not isinstance(value, (int, float))
                    or isinstance(value, bool)
                    or not math.isfinite(float(value))
                ):
                    errors.append(f"{task_id}: invalid sufficiency {field}")
        trust = row.get("trust_envelope")
        if not isinstance(trust, dict) or set(trust) != MEASUREMENT_TRUST_FIELDS:
            errors.append(f"{task_id}: invalid trust envelope projection")
        else:
            calibrated = trust["calibrated"]
            receipt_digest = trust["calibration_receipt_digest"]
            if (
                not isinstance(calibrated, bool)
                or trust["verdict"] != row.get("verdict")
                or (
                    calibrated
                    and (
                        not isinstance(receipt_digest, str)
                        or re.fullmatch(r"sha256:[0-9a-f]{64}", receipt_digest) is None
                    )
                )
                or (not calibrated and receipt_digest is not None)
                or (row.get("verdict") == "act" and not calibrated)
            ):
                errors.append(f"{task_id}: invalid trust/calibration binding")
            score = trust["score"]
            if score is not None and (
                not isinstance(score, (int, float))
                or isinstance(score, bool)
                or not math.isfinite(float(score))
            ):
                errors.append(f"{task_id}: invalid trust score")
    return errors


def _read_private_registry_json(
    path: pathlib.Path, registry_root: pathlib.Path
) -> dict[str, Any]:
    try:
        path.relative_to(registry_root)
    except ValueError as error:
        raise RunnerError("owner registry entry escapes its private root") from error
    _assert_no_symlink_components(path, "owner registry entry")
    root_stat = registry_root.lstat()
    if (
        not stat.S_ISDIR(root_stat.st_mode)
        or root_stat.st_uid != os.geteuid()
        or stat.S_IMODE(root_stat.st_mode) & 0o077
    ):
        raise RunnerError("owner registry root is not private")
    descriptor: int | None = None
    try:
        before_path = path.lstat()
        if stat.S_ISLNK(before_path.st_mode) or not stat.S_ISREG(before_path.st_mode):
            raise RunnerError("owner registry entry is not a regular file")
        descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
        before = os.fstat(descriptor)
        if (
            (before.st_dev, before.st_ino) != (before_path.st_dev, before_path.st_ino)
            or before.st_uid != os.geteuid()
            or before.st_size > MAX_PRIVATE_JSON_BYTES
        ):
            raise RunnerError("owner registry entry identity/owner/size is invalid")
        payload = os.read(descriptor, MAX_PRIVATE_JSON_BYTES + 1)
        after = os.fstat(descriptor)
        if (
            before.st_dev,
            before.st_ino,
            before.st_size,
            before.st_mtime_ns,
            before.st_ctime_ns,
        ) != (
            after.st_dev,
            after.st_ino,
            after.st_size,
            after.st_mtime_ns,
            after.st_ctime_ns,
        ):
            raise RunnerError("owner registry entry changed while reading")
    except RunnerError:
        raise
    except OSError as error:
        raise RunnerError("owner registry entry could not be read safely") from error
    finally:
        if descriptor is not None:
            os.close(descriptor)
    if len(payload) > MAX_PRIVATE_JSON_BYTES:
        raise RunnerError("owner registry entry exceeds its bounded size")
    try:
        value = json.loads(payload.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RunnerError("owner registry entry is invalid JSON") from error
    if not isinstance(value, dict):
        raise RunnerError("owner registry entry is not an object")
    return value


def _validate_registry_entry(
    entry: dict[str, Any],
    entry_path: pathlib.Path,
    *,
    process_pid: int,
    spec: OwnerSpec,
    launched_at_ms: int,
) -> dict[str, Any]:
    instance_id = _require_nonempty_string(
        entry.get("instance_id"), "owner registry instance_id"
    )
    if entry_path.name != f"{instance_id}.json":
        raise RunnerError("owner registry filename does not bind its instance_id")
    if entry.get("pid") != process_pid:
        raise RunnerError("owner registry entry belongs to a foreign PID")
    started_at_ms = _require_u64(
        entry.get("started_at_ms"), "owner registry started_at_ms"
    )
    if started_at_ms < launched_at_ms:
        raise RunnerError("owner registry entry predates the spawned process")
    port = entry.get("port")
    if (
        not isinstance(port, int)
        or isinstance(port, bool)
        or not 1 <= port <= 65_535
        or port == INSTALLED_OWNER_PORT
        or entry.get("bind") != "127.0.0.1"
    ):
        raise RunnerError("owner registry endpoint is invalid or forbidden")
    if (
        entry.get("mode") != "read_write"
        or entry.get("status") != "running"
        or entry.get("owner_live") is not True
        or entry.get("stale") is not False
        or entry.get("conflicts") != []
        or not _same_root(entry.get("workspace_root"), spec.root)
        or not _same_root(entry.get("runtime_root"), spec.runtime_dir)
    ):
        raise RunnerError("owner registry entry does not bind the spawned owner")
    graph_source = entry.get("graph_source")
    if not isinstance(graph_source, str):
        raise RunnerError("owner registry graph source is absent")
    graph_path = pathlib.Path(graph_source).resolve()
    try:
        graph_path.relative_to(spec.runtime_dir.resolve())
    except ValueError as error:
        raise RunnerError("owner registry graph source escapes the runtime") from error
    return entry


def _discover_owner_registry_entry(
    process: subprocess.Popen[Any],
    spec: OwnerSpec,
    *,
    launched_at_ms: int,
    timeout: float,
) -> tuple[pathlib.Path, dict[str, Any]]:
    instances = spec.registry_dir / "instances"
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RunnerError(
                f"owner exited before endpoint registration with code {process.returncode}"
            )
        if os.path.lexists(instances) and instances.is_symlink():
            raise RunnerError("owner registry instances directory is a symlink")
        try:
            final_entries = (
                sorted(path for path in instances.iterdir() if path.suffix == ".json")
                if instances.is_dir()
                else []
            )
        except OSError as error:
            raise RunnerError(
                "owner registry instances directory is unreadable"
            ) from error
        if len(final_entries) > 1:
            raise RunnerError("fresh owner registry contains multiple instances")
        if final_entries:
            entry_path = final_entries[0]
            entry = _read_private_registry_json(entry_path, spec.registry_dir)
            if entry.get("pid") != process.pid:
                raise RunnerError("fresh owner registry contains a foreign PID")
            if entry.get("status") == "running" and entry.get("port") is not None:
                return entry_path, _validate_registry_entry(
                    entry,
                    entry_path,
                    process_pid=process.pid,
                    spec=spec,
                    launched_at_ms=launched_at_ms,
                )
        time.sleep(0.05)
    raise RunnerError("owner did not publish its PID-bound endpoint")


def _wait_for_private_bearer(
    process: subprocess.Popen[Any], path: pathlib.Path, *, timeout: float
) -> CapturedBearerToken:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RunnerError("owner exited before publishing its bearer")
        if os.path.lexists(path):
            return capture_private_bearer(path)
        time.sleep(0.05)
    raise RunnerError("owner did not publish its private bearer")


def _validate_owner_attestation(
    spec: OwnerSpec,
    registry_entry: dict[str, Any],
    instance_response: dict[str, Any],
    manifest_response: dict[str, Any],
    *,
    binary_digest: str,
) -> OwnerReadinessReceipt:
    instance = instance_response.get("instance")
    graph_state = instance_response.get("graph_state")
    if not isinstance(instance, dict) or not isinstance(graph_state, dict):
        raise RunnerError("owner instance/self attestation is incomplete")
    for field_name in (
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
    ):
        if instance.get(field_name) != registry_entry.get(field_name):
            raise RunnerError(f"owner instance/self {field_name} differs from registry")
    if (
        instance.get("pid") <= 0
        or instance.get("mode") != "read_write"
        or instance.get("status") != "running"
        or instance.get("port") != spec.port
        or instance.get("bind") != "127.0.0.1"
        or not _same_root(instance.get("workspace_root"), spec.root)
        or not _same_root(instance.get("runtime_root"), spec.runtime_dir)
        or not _same_root(graph_state.get("runtime_root"), spec.runtime_dir)
        or not _same_root(graph_state.get("workspace_root"), spec.root)
        or graph_state.get("workspace_root_source") != "env:M1ND_WORKSPACE_ROOT"
    ):
        raise RunnerError("owner instance/self escaped its expected isolation")
    manifest = manifest_response.get("manifest")
    verification = manifest_response.get("verification")
    if (
        manifest_response.get("schema") != "m1nd-organism-manifest-response-v1"
        or not isinstance(manifest, dict)
        or not isinstance(verification, dict)
    ):
        raise RunnerError("owner manifest attestation is incomplete")
    manifest_digest = _require_digest(
        manifest.get("manifest_sha256"), "owner manifest digest", prefixed=False
    )
    computed = _rust_domain_digest(
        MANIFEST_DIGEST_DOMAIN, _without_key(manifest, "manifest_sha256")
    )
    if (
        manifest_digest != computed
        or verification.get("computed_manifest_sha256") != computed
    ):
        raise RunnerError("owner manifest self-digest is invalid")
    runtime = manifest.get("runtime")
    if not isinstance(runtime, dict):
        raise RunnerError("owner manifest runtime identity is absent")
    if (
        runtime.get("owner_id") != registry_entry["instance_id"]
        or runtime.get("started_at") != registry_entry["started_at_ms"]
        or runtime.get("binary_sha256") != binary_digest
    ):
        raise RunnerError("owner manifest does not bind PID/start/binary identity")
    return OwnerReadinessReceipt(
        instance_id=registry_entry["instance_id"],
        pid=registry_entry["pid"],
        started_at_ms=registry_entry["started_at_ms"],
        port=spec.port,
        registry_entry_digest=_sha256_bytes(_canonical_bytes(registry_entry)),
        manifest_digest=manifest_digest,
        binary_digest=binary_digest,
        token_captured_once=True,
        owner_binding_proven=True,
    )


def _wait_for_owner(
    process: subprocess.Popen[Any],
    spec: OwnerSpec,
    *,
    launched_at_ms: int,
    binary_digest: str,
    lane: str,
    timeout: float = 120.0,
) -> tuple[OwnerSpec, McpHttpClient, OwnerReadinessReceipt]:
    entry_path, entry = _discover_owner_registry_entry(
        process,
        spec,
        launched_at_ms=launched_at_ms,
        timeout=timeout,
    )
    del entry_path
    effective_spec = replace(spec, port=entry["port"])
    bearer = _wait_for_private_bearer(
        process, effective_spec.token_file, timeout=timeout
    )
    client = McpHttpClient(
        effective_spec.base_url,
        effective_spec.root,
        bearer,
        f"m1nd10-g6-blind-{lane}-{effective_spec.owner_id}",
    )
    if process.poll() is not None:
        raise RunnerError("owner exited before authenticated attestation")
    instance_response, _instance_raw = client.get_json(
        "/api/instance/self", timeout=min(timeout, 30.0)
    )
    if process.poll() is not None:
        raise RunnerError("owner exited during instance attestation")
    manifest_response, _manifest_raw = client.get_json(
        "/api/manifest", timeout=min(timeout, 30.0)
    )
    if process.poll() is not None:
        raise RunnerError("owner exited during manifest attestation")
    readiness = _validate_owner_attestation(
        effective_spec,
        entry,
        instance_response,
        manifest_response,
        binary_digest=binary_digest,
    )
    client.initialize()
    if process.poll() is not None:
        raise RunnerError("owner exited during MCP initialization")
    return effective_spec, client, readiness


def _start_owner(
    spec: OwnerSpec, binary: pathlib.Path, lane: str, binary_digest: str
) -> OwnerHandle:
    spec.runtime_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    spec.registry_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    spec.runtime_dir.chmod(0o700)
    spec.registry_dir.chmod(0o700)
    environment = _minimal_subprocess_env()
    environment["M1ND_WORKSPACE_ROOT"] = str(spec.root)
    command = [
        str(binary.resolve()),
        "--serve",
        "--port",
        "0",
        "--bind",
        "127.0.0.1",
        "--no-gui",
        "--runtime-dir",
        str(spec.runtime_dir),
        "--registry-dir",
        str(spec.registry_dir),
    ]
    owner_log = spec.log_path.open("x", encoding="utf-8")
    spec.log_path.chmod(0o600)
    client: McpHttpClient | None = None
    try:
        launched_at_ms = int(time.time_ns() // 1_000_000)
        process = subprocess.Popen(
            command,
            cwd=spec.root,
            env=environment,
            stdout=owner_log,
            stderr=subprocess.STDOUT,
            **_process_group_popen_kwargs(),
        )
    except BaseException:
        owner_log.close()
        raise
    try:
        effective_spec, client, readiness = _wait_for_owner(
            process,
            spec,
            launched_at_ms=launched_at_ms,
            binary_digest=binary_digest,
            lane=lane,
        )
        assert client.session_id is not None
        return OwnerHandle(
            spec=effective_spec,
            process=process,
            log=owner_log,
            client=client,
            initial_session_id=client.session_id,
            readiness=readiness,
        )
    except BaseException:
        if client is not None and client.session_id is not None:
            try:
                client.delete_session()
            except RunnerError:
                pass
        _terminate_process_group(process)
        owner_log.close()
        raise


def _stop_owner(handle: OwnerHandle) -> dict[str, Any]:
    report: dict[str, Any] = {
        "repo_id": handle.spec.repo_id,
        "owner_id": handle.spec.owner_id,
        "session_id": handle.initial_session_id,
        "same_session_for_owner_lifetime": (
            handle.client.session_id == handle.initial_session_id
        ),
        "session_delete_proven": False,
    }
    try:
        if handle.client.session_id != handle.initial_session_id:
            report["session_delete_error"] = "owner MCP session identity drifted"
        else:
            try:
                report.update(handle.client.delete_session())
            except RunnerError as error:
                report["session_delete_error"] = str(error)
    finally:
        try:
            report.update(_terminate_process_group(handle.process))
        finally:
            handle.log.close()
    report["cleanup_complete"] = bool(
        report["same_session_for_owner_lifetime"]
        and report["session_delete_proven"]
        and report["process_group_terminated"]
    )
    return report


def _call_tool_on_owner(
    handle: OwnerHandle, name: str, arguments: dict[str, Any]
) -> dict[str, Any]:
    if handle.process.poll() is not None:
        raise RunnerError(f"{handle.spec.repo_id} owner exited before {name}")
    if not handle.readiness.owner_binding_proven:
        raise RunnerError(f"{handle.spec.repo_id} owner readiness is not proven")
    if handle.client.session_id != handle.initial_session_id:
        raise RunnerError(f"{handle.spec.repo_id} owner session drifted before {name}")
    result = handle.client.call_tool(name, arguments)
    if handle.process.poll() is not None:
        raise RunnerError(f"{handle.spec.repo_id} owner exited during {name}")
    if handle.client.session_id != handle.initial_session_id:
        raise RunnerError(f"{handle.spec.repo_id} owner session drifted during {name}")
    return result


def build_result_artifact(
    *,
    queries: dict[str, Any],
    lane: str,
    run_id: str,
    system_revision: str,
    sealed_corpus_self_digest: str,
    metric_spec_digest: str,
    runner_digest: str,
    binary_digest: str,
    measurements: list[dict[str, Any]],
    run_metadata: dict[str, Any],
) -> dict[str, Any]:
    """Build the exact scorer-facing v2 result and seal its canonical digest."""
    for label, value in (
        ("sealed corpus self digest", sealed_corpus_self_digest),
        ("metric spec digest", metric_spec_digest),
        ("runner digest", runner_digest),
        ("binary digest", binary_digest),
    ):
        _require_digest(value, label, prefixed=True)
    if lane not in {"current", "baseline"}:
        raise RunnerError("result lane is invalid")
    _require_nonempty_string(run_id, "run_id")
    _require_nonempty_string(system_revision, "system_revision")
    manifest = queries["source_manifest"]
    artifact = {
        "schema": RESULT_SCHEMA,
        "lane": lane,
        "run_id": run_id,
        "corpus_id": queries["corpus_id"],
        "corpus_digest": queries["corpus_digest"],
        "public_corpus_self_digest": queries["self_digest"],
        "sealed_corpus_self_digest": sealed_corpus_self_digest,
        "source_manifest_digest": manifest["manifest_digest"],
        "source_revision": manifest["source_commit"],
        "system_revision": system_revision,
        "binary_digest": binary_digest,
        "runner_digest": runner_digest,
        "metric_spec_digest": metric_spec_digest,
        "measurements": measurements,
        "run_metadata": run_metadata,
    }
    artifact["self_digest"] = _self_digest(artifact)
    return artifact


def build_runner_checkpoint(
    *,
    queries: dict[str, Any],
    lane: str,
    run_id: str,
    system_revision: str,
    sealed_corpus_self_digest: str,
    metric_spec_digest: str,
    runner_digest: str,
    binary_digest: str,
    governed_ingests: list[dict[str, Any]],
    measurements: list[dict[str, Any]],
) -> dict[str, Any]:
    """Build a durable evidence checkpoint; it is explicitly not resumable."""
    bindings = [
        {
            "repo_id": row["repo_id"],
            "owner_id": row["owner_id"],
            "source_revision": row["source_revision"],
            "file_set_digest": row["file_set_digest"],
            "mcp_session_id": row["mcp_session_id"],
            "candidate_ownership_digest": row["candidate_ownership_digest"],
            "candidate_source_projection_digest": row[
                "candidate_source_projection_digest"
            ],
            "candidate_pipeline_digest": row["candidate_pipeline_digest"],
            "checkpoint_id": row["mutation_proof"]["checkpoint_id"],
            "checkpoint_generation": row["mutation_proof"]["checkpoint_generation"],
            "current_pointer_digest": row["mutation_proof"]["current_pointer_digest"],
        }
        for row in governed_ingests
    ]
    checkpoint = {
        "schema": RUNNER_CHECKPOINT_SCHEMA,
        "lane": lane,
        "run_id": run_id,
        "corpus_id": queries["corpus_id"],
        "corpus_digest": queries["corpus_digest"],
        "public_corpus_self_digest": queries["self_digest"],
        "source_manifest_digest": queries["source_manifest"]["manifest_digest"],
        "source_revision": queries["source_manifest"]["source_commit"],
        "system_revision": system_revision,
        "binary_digest": binary_digest,
        "runner_digest": runner_digest,
        "metric_spec_digest": metric_spec_digest,
        "sealed_corpus_self_digest": sealed_corpus_self_digest,
        "owner_ingest_bindings": bindings,
        "completed": len(measurements),
        "task_count": len(queries["tasks"]),
        "measurement_task_ids": [row["task_id"] for row in measurements],
        "measurements": measurements,
        "resume_supported": False,
        "generated_at": _utc_now(),
    }
    checkpoint["self_digest"] = _self_digest(checkpoint)
    validate_runner_checkpoint(checkpoint)
    return checkpoint


def validate_runner_checkpoint(checkpoint: Any) -> dict[str, Any]:
    checkpoint = _require_exact_fields(
        checkpoint, RUNNER_CHECKPOINT_FIELDS, "runner checkpoint"
    )
    if (
        checkpoint["schema"] != RUNNER_CHECKPOINT_SCHEMA
        or checkpoint["resume_supported"] is not False
        or checkpoint["self_digest"] != _self_digest(checkpoint)
    ):
        raise RunnerError("runner checkpoint identity/resume contract is invalid")
    measurements = checkpoint["measurements"]
    task_ids = checkpoint["measurement_task_ids"]
    if (
        not isinstance(measurements, list)
        or not isinstance(task_ids, list)
        or task_ids
        != [row.get("task_id") for row in measurements if isinstance(row, dict)]
        or len(task_ids) != len(set(task_ids))
        or checkpoint["completed"] != len(measurements)
        or not isinstance(checkpoint["task_count"], int)
        or checkpoint["task_count"] < checkpoint["completed"]
    ):
        raise RunnerError("runner checkpoint measurement coverage is invalid")
    bindings = checkpoint["owner_ingest_bindings"]
    if not isinstance(bindings, list) or not bindings:
        raise RunnerError("runner checkpoint has no owner ingest identity binding")
    for binding in bindings:
        binding = _require_exact_fields(
            binding, CHECKPOINT_OWNER_BINDING_FIELDS, "runner checkpoint owner binding"
        )
        for field in (
            "candidate_ownership_digest",
            "candidate_source_projection_digest",
            "candidate_pipeline_digest",
            "current_pointer_digest",
        ):
            _require_digest(binding.get(field), f"checkpoint {field}", prefixed=False)
        _require_nonempty_string(
            binding.get("mcp_session_id"), "checkpoint MCP session"
        )
        _require_nonempty_string(binding.get("checkpoint_id"), "checkpoint ACK id")
        _require_nonnegative_integer(
            binding.get("checkpoint_generation"), "checkpoint ACK generation"
        )
    return checkpoint


def _run(args: argparse.Namespace) -> dict[str, Any]:
    path_topology = validate_runner_paths(args)
    queries = json.loads(args.queries.read_text(encoding="utf-8"))
    tasks = validate_public_queries(queries)
    metric_spec = json.loads(args.metric_spec.read_text(encoding="utf-8"))
    calibration_spec = validate_metric_spec_for_runner(metric_spec)
    _require_digest(
        args.sealed_corpus_self_digest,
        "sealed corpus self digest",
        prefixed=True,
    )
    _require_nonempty_string(args.run_id, "run_id")
    binary_digest = _sha256(args.binary)
    metric_spec_digest = _sha256(args.metric_spec)
    runner_digest = _sha256(pathlib.Path(__file__).resolve())

    provider: AuthorityProvider | None = None
    if args.authority_provider is not None:
        provider = ExternalAuthorityProvider(
            args.authority_provider, timeout=args.authority_provider_timeout
        )
    if provider is None:
        mode = "diagnostic" if args.diagnostic else "formal"
        raise RunnerError(f"external authority provider is required for {mode} runs")
    if args.authority_assembly is None:
        raise RunnerError("an independently pinned authority assembly is required")
    assembly_document = json.loads(args.authority_assembly.read_text(encoding="utf-8"))
    assembly = load_authority_assembly(
        assembly_document,
        expected_digest=args.expected_authority_assembly_digest,
        binary_digest=binary_digest,
        provider_executable_digest=provider.identity_digest,
        blind_boundary_kind=provider.blind_boundary_kind,
        blind_boundary_proven=provider.blind_boundary_proven,
    )
    assembly = preflight_authority_provider(
        provider,
        assembly,
        diagnostic=args.diagnostic,
        lane=args.lane,
        binary_digest=binary_digest,
    )
    verifier = BinaryAuthorizationReceiptVerifier(args.binary, binary_digest)

    specs = build_owner_specs(
        queries,
        args.source_root,
        args.runtime_dir,
        args.registry_dir,
        args.base_port,
    )
    source_verification = verify_public_source_snapshot(queries, args.source_root)
    handles: list[OwnerHandle] = []
    handles_by_repo: dict[str, OwnerHandle] = {}
    measurements: list[dict[str, Any]] = []
    calibration_receipts: list[dict[str, Any] | None] = []
    raw_verdicts: Counter[str] = Counter()
    governed_ingests: list[dict[str, Any]] = []
    post_ingest_source_verification: dict[str, Any] | None = None
    cleanup_reports: list[dict[str, Any]] = []
    errors: list[dict[str, str]] = []
    started_at = _utc_now()
    try:
        for spec in specs:
            handle = _start_owner(spec, args.binary, args.lane, binary_digest)
            handles.append(handle)
            handles_by_repo[spec.repo_id] = handle

        for spec in specs:
            governed_ingests.append(
                execute_governed_graph_ingest(
                    spec,
                    handles_by_repo[spec.repo_id].client,
                    provider,
                    assembly,
                    verifier,
                    diagnostic=args.diagnostic,
                    lane=args.lane,
                    binary_digest=binary_digest,
                )
            )

        post_ingest_source_verification = verify_public_source_snapshot(
            queries, args.source_root
        )
        if post_ingest_source_verification != source_verification:
            raise RunnerError("public source snapshot changed across governed ingest")

        for spec in specs:
            handle = handles_by_repo[spec.repo_id]
            warm_agent = f"g6-blind-{args.lane}-warm-{spec.repo_id}"
            warm_query = "Locate source-code structure for retrieval warm-up."
            _call_tool_on_owner(
                handle,
                "north",
                {
                    "agent_id": warm_agent,
                    "task": warm_query,
                    "tier": "project",
                    "top_k": 5,
                },
            )
            _call_tool_on_owner(
                handle,
                "seek",
                {
                    "agent_id": warm_agent,
                    "query": warm_query,
                    "tier": "project",
                    "top_k": 5,
                    "min_score": 0.1,
                    "graph_rerank": True,
                },
            )

        for index, task in enumerate(tasks, start=1):
            task_id = task["task_id"]
            repo_id = task["repo_id"]
            handle = handles_by_repo[repo_id]
            agent_id = f"g6-blind-{args.lane}-{task_id}"
            try:
                started = time.perf_counter_ns()
                _call_tool_on_owner(
                    handle,
                    "north",
                    {
                        "agent_id": agent_id,
                        "task": task["query"],
                        "tier": "project",
                        "top_k": 5,
                    },
                )
                north_latency_ms = (time.perf_counter_ns() - started) / 1_000_000

                started = time.perf_counter_ns()
                seek_result = _call_tool_on_owner(
                    handle,
                    "seek",
                    {
                        "agent_id": agent_id,
                        "query": task["query"],
                        "tier": "project",
                        "top_k": 5,
                        "min_score": 0.1,
                        "graph_rerank": True,
                    },
                )
                seek_latency_ms = (time.perf_counter_ns() - started) / 1_000_000
                measurement, _raw_verdict, calibration_receipt = extract_measurement(
                    task_id, north_latency_ms, seek_latency_ms, seek_result
                )
                raw_verdicts[measurement["verdict"]] += 1
            except RunnerError as error:
                errors.append({"task_id": task_id, "error": str(error)})
                raise RunnerError(
                    f"task {task_id} failed; refusing to fabricate a latency or abstention"
                ) from error
            measurements.append(measurement)
            calibration_receipts.append(calibration_receipt)

            if args.checkpoint and (index % 10 == 0 or index == len(tasks)):
                checkpoint = build_runner_checkpoint(
                    queries=queries,
                    lane=args.lane,
                    run_id=args.run_id,
                    system_revision=args.system_revision,
                    sealed_corpus_self_digest=args.sealed_corpus_self_digest,
                    metric_spec_digest=metric_spec_digest,
                    runner_digest=runner_digest,
                    binary_digest=binary_digest,
                    governed_ingests=governed_ingests,
                    measurements=measurements,
                )
                _write_json_durable(args.checkpoint, checkpoint)
            if args.progress_every and (
                index % args.progress_every == 0 or index == len(tasks)
            ):
                print(
                    json.dumps(
                        {
                            "lane": args.lane,
                            "completed": index,
                            "task_count": len(tasks),
                            "errors": len(errors),
                        },
                        sort_keys=True,
                    ),
                    flush=True,
                )
    finally:
        for handle in reversed(handles):
            try:
                cleanup_reports.append(_stop_owner(handle))
            except BaseException as error:
                cleanup_reports.append(
                    {
                        "repo_id": handle.spec.repo_id,
                        "owner_id": handle.spec.owner_id,
                        "cleanup_complete": False,
                        "cleanup_error": type(error).__name__,
                    }
                )

    if len(cleanup_reports) != len(specs) or not all(
        report.get("cleanup_complete") is True for report in cleanup_reports
    ):
        raise RunnerError("owner session/process-group cleanup was not fully proven")

    authority_metadata = authority_run_metadata(
        diagnostic=args.diagnostic, assembly=assembly
    )
    calibration = build_calibration_summary(
        calibration_receipts, measurements, calibration_spec
    )
    authority_receipts_proven = bool(governed_ingests) and all(
        row.get("production_authority_receipt_proven") is True
        for row in governed_ingests
    )
    authority_metadata["production_authority_assembly_proven"] = (
        authority_receipts_proven
    )
    cleanup_by_repo = {row.get("repo_id"): row for row in cleanup_reports}
    owner_metadata = [
        {
            "repo_id": handle.spec.repo_id,
            "owner_id": handle.spec.owner_id,
            "instance_id": handle.readiness.instance_id,
            "source_revision": handle.spec.source_revision,
            "file_set_digest": handle.spec.file_set_digest,
            "source_root": str(handle.spec.root),
            "port": handle.spec.port,
            "runtime_dir": str(handle.spec.runtime_dir),
            "registry_dir": str(handle.spec.registry_dir),
            "process_isolated": handle.readiness.owner_binding_proven,
            "mcp_session_isolated": handle.readiness.owner_binding_proven,
            "readiness": {
                "pid": handle.readiness.pid,
                "started_at_ms": handle.readiness.started_at_ms,
                "registry_entry_digest": handle.readiness.registry_entry_digest,
                "manifest_digest": handle.readiness.manifest_digest,
                "binary_digest": handle.readiness.binary_digest,
                "token_captured_once": handle.readiness.token_captured_once,
                "owner_binding_proven": handle.readiness.owner_binding_proven,
            },
            "mcp_session_id": next(
                row["mcp_session_id"]
                for row in governed_ingests
                if row["repo_id"] == handle.spec.repo_id
            ),
            "cleanup": cleanup_by_repo[handle.spec.repo_id],
        }
        for handle in handles
    ]
    checkpoint_proof: dict[str, Any]
    if args.checkpoint is None:
        checkpoint_proof = {
            "enabled": False,
            "resume_supported": False,
            "claim": "no resume or checkpoint claim",
        }
    else:
        final_checkpoint = validate_runner_checkpoint(
            json.loads(args.checkpoint.read_text(encoding="utf-8"))
        )
        checkpoint_proof = {
            "enabled": True,
            "schema": final_checkpoint["schema"],
            "self_digest": final_checkpoint["self_digest"],
            "completed": final_checkpoint["completed"],
            "resume_supported": False,
            "durable_atomic_write": True,
        }
    formal_complete = bool(
        not args.diagnostic
        and authority_receipts_proven
        and source_verification["exact_live_file_set"] is True
        and post_ingest_source_verification == source_verification
        and assembly.blind_boundary_proven
        and assembly.expected_digest_verified
        and all(handle.readiness.owner_binding_proven for handle in handles)
        and all(row["cleanup_complete"] is True for row in cleanup_reports)
        and path_topology["disjoint"] is True
    )
    missing: list[str] = []
    if args.diagnostic:
        missing.append("formal mode was not requested")
    if not authority_receipts_proven:
        missing.append("production authority receipt/signer assembly")
    if not assembly.blind_boundary_proven:
        missing.append("authority provider blind filesystem boundary")
    if not all(handle.readiness.owner_binding_proven for handle in handles):
        missing.append("PID/registry/bearer/manifest owner binding")
    if post_ingest_source_verification != source_verification:
        missing.append("post-ingest source snapshot identity")
    formal_preflights = {
        "complete": formal_complete,
        "status": "PROVEN" if formal_complete else PROOF_NOT_PROVEN,
        "missing": missing,
        "delivery": "delivery-2-hardened-runner",
        "same_session_readiness_ingest_measurement_delete": all(
            row["same_session_for_owner_lifetime"] is True
            and row["session_delete_proven"] is True
            for row in cleanup_reports
        ),
        "process_group_cleanup": all(
            row["process_group_terminated"] is True for row in cleanup_reports
        ),
        "source_live_identity": source_verification["exact_live_file_set"],
        "source_post_ingest_identity": post_ingest_source_verification
        == source_verification,
        "authority_blind_boundary": {
            "kind": assembly.blind_boundary_kind,
            "proven": assembly.blind_boundary_proven,
        },
        "owner_readiness_bindings_proven": all(
            handle.readiness.owner_binding_proven for handle in handles
        ),
        "path_topology": path_topology,
        "authority_receipts_proven": authority_receipts_proven,
        "checkpoint": checkpoint_proof,
    }
    score_eligible = bool(
        formal_complete
        and calibration["status"] == "armed"
        and not errors
        and len(measurements) == len(tasks)
    )
    run_metadata = {
        "schema": RUN_METADATA_SCHEMA,
        "lane": args.lane,
        "run_id": args.run_id,
        "generated_at": _utc_now(),
        "started_at": started_at,
        "transport": "mcp-http-loopback",
        "task_count": len(tasks),
        "unscored": True,
        "score_eligible": score_eligible,
        "diagnostic_only": not score_eligible,
        "proof_state": "PROVEN" if score_eligible else PROOF_NOT_PROVEN,
        "formal_preflights": formal_preflights,
        **authority_metadata,
        "labels_read": False if assembly.blind_boundary_proven else PROOF_NOT_PROVEN,
        "actions_executed": 0,
        "benchmark_task_actions_executed": 0,
        "governed_setup_mutations_executed": len(governed_ingests),
        "verdict_mapping": (
            "canonical runtime verdict preserved; missing canonical verdict maps "
            "to abstain for zero candidates, otherwise reverify; act is never synthesized"
        ),
        "raw_runtime_verdict_counts": dict(sorted(raw_verdicts.items())),
        "calibration": calibration,
        "source_verification": source_verification,
        "post_ingest_source_verification": post_ingest_source_verification,
        "owner_topology": owner_metadata,
        "owner_cleanup": cleanup_reports,
        "governed_graph_ingest": governed_ingests,
        "warmup": {
            "per_repo": True,
            "north_calls": len(specs),
            "seek_calls": len(specs),
            "measured": False,
        },
        "errors": errors,
    }
    artifact = build_result_artifact(
        queries=queries,
        lane=args.lane,
        run_id=args.run_id,
        system_revision=args.system_revision,
        sealed_corpus_self_digest=args.sealed_corpus_self_digest,
        metric_spec_digest=metric_spec_digest,
        runner_digest=runner_digest,
        binary_digest=binary_digest,
        measurements=measurements,
        run_metadata=run_metadata,
    )
    validation_errors = validate_unscored_artifact(artifact, queries)
    if validation_errors:
        raise RunnerError("result validation failed: " + "; ".join(validation_errors))
    _write_json_atomic(args.output, artifact)
    return artifact


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--queries", type=pathlib.Path, required=True)
    parser.add_argument("--metric-spec", type=pathlib.Path, required=True)
    parser.add_argument("--sealed-corpus-self-digest", required=True)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--source-root", type=pathlib.Path, required=True)
    parser.add_argument("--runtime-dir", type=pathlib.Path, required=True)
    parser.add_argument("--registry-dir", type=pathlib.Path, required=True)
    parser.add_argument(
        "--base-port",
        type=int,
        default=0,
        help="deprecated compatibility flag; owners always request kernel port 0",
    )
    parser.add_argument("--authority-provider", type=pathlib.Path)
    parser.add_argument("--authority-assembly", type=pathlib.Path)
    parser.add_argument("--expected-authority-assembly-digest", required=True)
    parser.add_argument("--authority-provider-timeout", type=float, default=30.0)
    parser.add_argument("--lane", choices=("current", "baseline"), required=True)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--system-revision", required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--checkpoint", type=pathlib.Path)
    parser.add_argument("--progress-every", type=int, default=20)
    parser.add_argument(
        "--diagnostic",
        action="store_true",
        help=(
            "permit software-test authority while permanently marking the run "
            "score_eligible=false and proof_state=NOT_PROVEN"
        ),
    )
    args = parser.parse_args(argv)
    try:
        artifact = _run(args)
    except (
        OSError,
        ValueError,
        TypeError,
        KeyError,
        json.JSONDecodeError,
        RunnerError,
    ) as error:
        print(
            json.dumps({"status": "ERROR", "error": str(error)}, sort_keys=True),
            file=sys.stderr,
        )
        return 1
    print(
        json.dumps(
            {
                "status": "OK",
                "lane": args.lane,
                "measurements": len(artifact["measurements"]),
                "errors": len(artifact["run_metadata"]["errors"]),
                "output": str(args.output),
                "binary_digest": artifact["binary_digest"],
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
