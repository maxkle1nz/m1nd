import copy
import importlib.util
import pathlib
import sys
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "benchmark" / "m1nd10_g6_retrieval.py"
SPEC = importlib.util.spec_from_file_location("m1nd10_g6_retrieval", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)

sys.path.insert(0, str(ROOT / "scripts" / "benchmark"))
import m1nd10_g6_blind_runner as RUNNER  # noqa: E402


def digest(label: str) -> str:
    return MODULE._sha256_bytes(label.encode("utf-8"))


def seal(value: dict) -> dict:
    value["self_digest"] = MODULE._self_digest(value)
    return value


def metric_spec(seek_slo=1000) -> dict:
    return seal(
        {
            "schema": MODULE.SPEC_SCHEMA,
            "version": 2,
            "ratification": {
                "status": "ratified",
                "outcome_blind": True,
                "authority_receipt_digest": digest("metric-authority"),
                "unratified_fields": [],
            },
            "corpus": {
                "minimum_tasks": 200,
                "minimum_languages": 2,
                "minimum_repo_size_bands": 2,
                "minimum_localizable": 1,
                "minimum_unlocalizable": 1,
            },
            "thresholds": {
                "top5_anchor_recall_min": 0.9,
                "abstention_recall_min": 0.95,
                "wrong_ground_action_rate_max": 0.01,
                "regression_significance_alpha": 0.05,
            },
            "latency_slo_ms": {"north_p95": 2000, "seek_p95": seek_slo},
            "measurement_integrity": {
                "require_error_free_runner_metadata": True,
                "reject_error_fallback_measurements": True,
                "minimum_executed_latency_ms_exclusive": 0.000001,
                "include_fresh_session_overhead": True,
                "require_result_self_digest": True,
                "require_exact_lane_binding": True,
                "require_baseline_ratification_receipt": True,
                "require_sealed_run_ledger": True,
                "same_revision_rerun_policy": "one_sealed_run_only_no_rerun_until_pass",
            },
            "calibration": {
                "minimum_calibration_sample_size": 30,
                "minimum_calibration_precision": 0.99,
                "minimum_calibration_coverage": 0.1,
                "minimum_calibrated_task_fraction": 1.0,
                "minimum_authorized_action_count": 10,
            },
        }
    )


def source_manifest() -> dict:
    identities = (
        ("m1nd-mcp", "sources/m1nd-mcp", "rust", 120_000, "src/lib.rs"),
        ("m1nd-core", "sources/m1nd-core", "rust", 25_000, "src/lib.rs"),
        (
            "m1nd-python-tools",
            "sources/m1nd-python-tools",
            "python",
            5_000,
            "m1nd/__init__.py",
        ),
        ("m1nd-ui", "sources/m1nd-ui", "typescript", 30_000, "src/index.ts"),
    )
    repos = []
    for index, (repo_id, source_root, language, lines, path) in enumerate(identities):
        tree = f"{index + 1:040x}"
        files = [
            {
                "path": path,
                "role": "source",
                "bytes": lines * 8,
                "lines": lines,
                "sha256": digest(f"source-file-{index}"),
            }
        ]
        repos.append(
            {
                "repo_id": repo_id,
                "source_root": source_root,
                "source_revision": f"git:{MODULE.V2_SOURCE_COMMIT}:tree:{tree}",
                "git_tree": tree,
                "primary_language": language,
                "repo_size_band": MODULE._size_band(lines),
                "size_band_definition": dict(MODULE.SIZE_BAND_DEFINITION),
                "source_file_count": 1,
                "source_line_count": lines,
                "searched_file_count": 1,
                "files": files,
                "file_set_digest": MODULE._sha256_bytes(MODULE._canonical_bytes(files)),
            }
        )
    manifest = {
        "schema": MODULE.SOURCE_MANIFEST_SCHEMA,
        "source_commit": MODULE.V2_SOURCE_COMMIT,
        "snapshot_kind": "immutable_git_objects",
        "worktree_state_excluded": True,
        "repos": repos,
    }
    manifest["manifest_digest"] = MODULE._sha256_bytes(
        MODULE._canonical_bytes(manifest)
    )
    return manifest


def corpora() -> tuple[dict, dict]:
    manifest = source_manifest()
    sealed_tasks = []
    prefixes = ("mcp", "core", "py", "ui")
    for index in range(220):
        localizable = index < 200
        repo = index % 4
        task_id = f"g6-{prefixes[repo]}-{index:016x}"
        anchor = f"file::src/anchor_{index}.rs"
        repo_row = manifest["repos"][repo]
        sealed_tasks.append(
            {
                "task_id": task_id,
                "repo_id": repo_row["repo_id"],
                "repo_revision": repo_row["source_revision"],
                "language": repo_row["primary_language"],
                "repo_size_band": repo_row["repo_size_band"],
                "query": (
                    "Which pinned source symbol implements synthetic retrieval "
                    f"behavior number {index:03d}?"
                ),
                "localizable": localizable,
                "accepted_anchor_ids": [anchor] if localizable else [],
            }
        )
    public_tasks = [
        {field: task[field] for field in MODULE.PUBLIC_TASK_FIELDS}
        for task in sealed_tasks
    ]
    corpus_digest = MODULE._sha256_bytes(
        MODULE._canonical_bytes(
            {
                "source_manifest_digest": manifest["manifest_digest"],
                "tasks": public_tasks,
            }
        )
    )
    corpus_id = "m1nd10-g6-held-out-v2-" + corpus_digest.removeprefix("sha256:")[:16]
    public = seal(
        {
            "schema": MODULE.PUBLIC_SCHEMA,
            "version": 2,
            "corpus_id": corpus_id,
            "corpus_digest": corpus_digest,
            "blinded": True,
            "author_review_status": MODULE.V2_AUTHOR_STATUS,
            "source_manifest": manifest,
            "task_count": len(public_tasks),
            "runner_contract": {
                "read_only_artifact": "public/queries.json",
                "forbidden_artifact": "operator-only/corpus.json",
                "result_coverage": "emit exactly one measurement for every task_id",
                "source_checkout": MODULE.V2_SOURCE_COMMIT,
                "labels_exposed": False,
                "independent_review_status": "NOT_RUN",
            },
            "tasks": public_tasks,
        }
    )
    corpus = seal(
        {
            "schema": MODULE.CASE_SCHEMA,
            "version": 2,
            "corpus_id": corpus_id,
            "corpus_digest": corpus_digest,
            "blinded": True,
            "adjudication_sealed_at": MODULE.V2_SEALED_AT,
            "author_review_status": MODULE.V2_AUTHOR_STATUS,
            "source_manifest": manifest,
            "counts": {
                "total": 220,
                "localizable": 200,
                "unlocalizable": 20,
                "by_language": dict(
                    sorted(
                        MODULE.Counter(
                            task["language"] for task in sealed_tasks
                        ).items()
                    )
                ),
                "by_repo_size_band": dict(
                    sorted(
                        MODULE.Counter(
                            task["repo_size_band"] for task in sealed_tasks
                        ).items()
                    )
                ),
                "by_repo": dict(
                    sorted(
                        MODULE.Counter(task["repo_id"] for task in sealed_tasks).items()
                    )
                ),
            },
            "methodology": {"fixture": "held-out-v2-compatibility"},
            "tasks": sealed_tasks,
        }
    )
    return public, corpus


def formal_run_metadata(
    lane: str,
    public: dict,
    binary_digest: str,
    receipt_digest: str,
) -> dict:
    manifest = public["source_manifest"]
    base = f"/formal-{lane}"
    source_root = f"{base}/source"
    runtime_root = f"{base}/runtime"
    registry_root = f"{base}/registry"
    cleanups = []
    topologies = []
    ingests = []
    repo_roots = {}
    key_id = "authority-key-1"
    checked_at_ms = 1_721_376_000_000
    for index, repo in enumerate(manifest["repos"]):
        repo_id = repo["repo_id"]
        owner_id = f"owner-{lane}-{index}"
        session_id = f"session-{lane}-{index}"
        repo_root = f"{source_root}/{repo['source_root']}"
        repo_roots[repo_id] = repo_root
        cleanup = {
            "repo_id": repo_id,
            "same_session_for_owner_lifetime": True,
            "session_delete_proven": True,
            "process_group_terminated": True,
            "cleanup_complete": True,
        }
        cleanups.append(cleanup)
        topologies.append(
            {
                "repo_id": repo_id,
                "owner_id": owner_id,
                "instance_id": f"instance-{lane}-{index}",
                "source_revision": repo["source_revision"],
                "file_set_digest": repo["file_set_digest"],
                "source_root": repo_root,
                "port": 49_152 + index,
                "runtime_dir": f"{runtime_root}/{repo_id}",
                "registry_dir": f"{registry_root}/{repo_id}",
                "process_isolated": True,
                "mcp_session_isolated": True,
                "readiness": {
                    "pid": 40_000 + index,
                    "started_at_ms": checked_at_ms,
                    "registry_entry_digest": digest(f"registry-{lane}-{repo_id}"),
                    "manifest_digest": digest(f"manifest-{lane}-{repo_id}"),
                    "binary_digest": binary_digest,
                    "token_captured_once": True,
                    "owner_binding_proven": True,
                },
                "mcp_session_id": session_id,
                "cleanup": copy.deepcopy(cleanup),
            }
        )
        ingests.append(
            {
                "repo_id": repo_id,
                "owner_id": owner_id,
                "source_revision": repo["source_revision"],
                "file_set_digest": repo["file_set_digest"],
                "semantic_payload_digest": digest(f"semantic-{lane}-{repo_id}"),
                "operation_object_digest": digest(f"operation-{lane}-{repo_id}"),
                "mcp_session_id": session_id,
                "candidate_ownership_digest": digest(f"ownership-{lane}-{repo_id}"),
                "candidate_source_projection_digest": digest(
                    f"projection-{lane}-{repo_id}"
                ),
                "candidate_pipeline_digest": digest(f"pipeline-{lane}-{repo_id}"),
                "authorization_lease_bound": True,
                "authority_receipt": {
                    "authority_variant": "POSITIVE_SOVEREIGN",
                    "control_verified_ed25519": True,
                    "receipt_core_digest_verified": True,
                    "assembly_digest_verified": True,
                    "key_registry_epoch": 7,
                    "signature_verified": True,
                    "clock_verified": True,
                    "key_lifecycle_verified": True,
                    "checked_at_ms": checked_at_ms,
                    "receipt_signer_metadata_production": True,
                    "production_authority_receipt_proven": True,
                    "receipt_digest": digest(f"authority-{lane}-{repo_id}"),
                    "issuer": "production-authority",
                    "key_id": key_id,
                    "algorithm": "ED25519",
                },
                "production_authority_receipt_proven": True,
                "reconciliation_state": "RECONCILED",
                "files_scanned": len(repo["files"]),
                "files_parsed": len(repo["files"]),
                "node_count": len(repo["files"]),
                "edge_count": 0,
                "mutation_proof": {"checkpoint_ack_exact": True},
                "governed_ingest_latency_ms": 1.0,
            }
        )
    source_verification = {
        "checked_files": sum(len(repo["files"]) for repo in manifest["repos"]),
        "missing_files": 0,
        "digest_mismatches": 0,
        "extra_files": 0,
        "checked_bytes": sum(
            entry["bytes"] for repo in manifest["repos"] for entry in repo["files"]
        ),
        "checked_lines": sum(
            entry["lines"] for repo in manifest["repos"] for entry in repo["files"]
        ),
        "exact_live_file_set": True,
        "symlinks_rejected": True,
        "isolated_snapshot_required": True,
        "git_objects_used_as_live_root": False,
        "repo_roots": repo_roots,
    }
    path_topology = {
        "absolute": True,
        "fresh_mutable_roots": True,
        "disjoint": True,
        "symlink_free_path_components": True,
        "paths": {
            "source_root": source_root,
            "runtime_dir": runtime_root,
            "registry_dir": registry_root,
            "output": f"{base}/result.json",
        },
    }
    metadata = {
        "schema": MODULE.RUN_METADATA_SCHEMA,
        "lane": lane,
        "run_id": f"run-{lane}",
        "generated_at": "2026-07-19T00:00:00Z",
        "started_at": "2026-07-19T00:00:00Z",
        "transport": "mcp-http-loopback",
        "task_count": 220,
        "unscored": True,
        "score_eligible": True,
        "diagnostic_only": False,
        "proof_state": "PROVEN",
        "formal_preflights": {
            "complete": True,
            "status": "PROVEN",
            "missing": [],
            "delivery": "delivery-2-hardened-runner",
            "same_session_readiness_ingest_measurement_delete": True,
            "process_group_cleanup": True,
            "source_live_identity": True,
            "source_post_ingest_identity": True,
            "authority_blind_boundary": {
                "kind": "macos-sandbox-exec-deny-default",
                "proven": True,
            },
            "owner_readiness_bindings_proven": True,
            "path_topology": path_topology,
            "authority_receipts_proven": True,
            "checkpoint": {"enabled": False},
        },
        "authority_mode": "formal",
        "authority_provider_kind": "production",
        "authority_provider_claimed_production_assembly": True,
        "production_authority_assembly_proven": True,
        "authority_assembly_id": f"assembly-{lane}",
        "authority_assembly_digest": digest(f"assembly-{lane}").removeprefix("sha256:"),
        "authority_assembly_digest_verified": True,
        "authority_provider_executable_digest": digest(f"provider-{lane}"),
        "authority_owner_security_config_digest": digest(f"security-{lane}"),
        "authority_key_registry_epoch": 7,
        "authority_receipt_key_id": key_id,
        "authority_blind_boundary_kind": "macos-sandbox-exec-deny-default",
        "authority_blind_boundary_proven": True,
        "labels_read": False,
        "actions_executed": 0,
        "benchmark_task_actions_executed": 0,
        "governed_setup_mutations_executed": len(manifest["repos"]),
        "verdict_mapping": "runtime-trust-envelope",
        "raw_runtime_verdict_counts": {"abstain": 20, "act": 200},
        "calibration": {
            "schema": MODULE.CALIBRATION_SCHEMA,
            "status": "armed",
            "receipt_digest": receipt_digest,
            "receipt_schema": MODULE.SEEK_CALIBRATION_RECEIPT_SCHEMA,
            "signal": MODULE.SEEK_CALIBRATION_SIGNAL,
            "tau": 0.73,
            "sample_size": 100,
            "measured_precision": 1.0,
            "coverage": 0.9,
            "target_alpha": 0.01,
            "calibrated_at_ms": checked_at_ms,
            "calibrated_task_count": 220,
            "authorized_action_count": 200,
        },
        "source_verification": source_verification,
        "post_ingest_source_verification": copy.deepcopy(source_verification),
        "owner_topology": topologies,
        "owner_cleanup": cleanups,
        "governed_graph_ingest": ingests,
        "warmup": {"completed": True},
        "errors": [],
    }
    assert set(metadata) == MODULE.RUN_METADATA_FIELDS
    return metadata


def result(
    lane: str,
    public: dict,
    corpus: dict,
    metric_spec_digest: str,
    runner_digest: str,
    binary_digest: str,
) -> dict:
    receipt_digest = digest(f"calibration-{lane}")
    measurements = []
    for index, task in enumerate(corpus["tasks"]):
        localizable = task["localizable"]
        verdict = "act" if localizable else "abstain"
        measurements.append(
            {
                "task_id": task["task_id"],
                "ranked_anchor_ids": list(task["accepted_anchor_ids"]),
                "verdict": verdict,
                "north_latency_ms": 100 + index % 10,
                "seek_latency_ms": 80 + index % 10,
                "north_executed": True,
                "seek_executed": True,
                "trust_envelope": {
                    "calibrated": True,
                    "verdict": verdict,
                    "calibration_receipt_digest": receipt_digest,
                },
            }
        )
    return RUNNER.build_result_artifact(
        queries=public,
        lane=lane,
        run_id=f"run-{lane}",
        system_revision=f"system-{lane}",
        sealed_corpus_self_digest=corpus["self_digest"],
        metric_spec_digest=metric_spec_digest,
        runner_digest=runner_digest,
        binary_digest=binary_digest,
        measurements=measurements,
        run_metadata=formal_run_metadata(lane, public, binary_digest, receipt_digest),
    )


def baseline_receipt(bundle: dict) -> dict:
    baseline = bundle["baseline"]
    corpus = bundle["corpus"]
    public = bundle["public"]
    return seal(
        {
            "schema": MODULE.BASELINE_RECEIPT_SCHEMA,
            "version": 1,
            "status": "ratified",
            "outcome_blind": True,
            "selection_policy": "last owner-ratified release before candidate work",
            "authority": {
                "authority_id": "test-owner",
                "receipt_digest": digest("baseline-authority"),
            },
            "baseline": {
                "lane": "baseline",
                "run_id": baseline["run_id"],
                "result_self_digest": baseline["self_digest"],
                "corpus_id": corpus["corpus_id"],
                "corpus_digest": corpus["corpus_digest"],
                "public_corpus_self_digest": public["self_digest"],
                "sealed_corpus_self_digest": corpus["self_digest"],
                "source_manifest_digest": corpus["source_manifest"]["manifest_digest"],
                "metric_spec_digest": bundle["metric_spec_file_digest"],
                "runner_digest": bundle["runner_digest"],
                "system_revision": baseline["system_revision"],
                "binary_digest": bundle["baseline_binary_digest"],
            },
        }
    )


def ledger_entry(result_artifact: dict, sequence: int, previous: str | None) -> dict:
    entry = {
        "schema": MODULE.RUN_LEDGER_ENTRY_SCHEMA,
        "sequence": sequence,
        "previous_entry_digest": previous,
        **MODULE._ledger_binding(result_artifact),
    }
    entry["entry_digest"] = MODULE._entry_digest(entry)
    return entry


def run_ledger(bundle: dict) -> dict:
    baseline_entry = ledger_entry(bundle["baseline"], 1, None)
    current_entry = ledger_entry(bundle["current"], 2, baseline_entry["entry_digest"])
    return seal(
        {
            "schema": MODULE.RUN_LEDGER_SCHEMA,
            "version": 1,
            "ledger_id": "synthetic-ledger",
            "entry_count": 2,
            "entries": [baseline_entry, current_entry],
            "final_entry_digest": current_entry["entry_digest"],
        }
    )


def evidence() -> dict:
    spec = metric_spec()
    public, corpus = corpora()
    bundle = {
        "spec": spec,
        "public": public,
        "corpus": corpus,
        "metric_spec_file_digest": MODULE._sha256_bytes(MODULE._canonical_bytes(spec)),
        "runner_digest": digest("runner"),
        "current_binary_digest": digest("current-binary"),
        "baseline_binary_digest": digest("baseline-binary"),
    }
    bundle["current"] = result(
        "current",
        public,
        corpus,
        bundle["metric_spec_file_digest"],
        bundle["runner_digest"],
        bundle["current_binary_digest"],
    )
    bundle["baseline"] = result(
        "baseline",
        public,
        corpus,
        bundle["metric_spec_file_digest"],
        bundle["runner_digest"],
        bundle["baseline_binary_digest"],
    )
    bundle["baseline_receipt"] = baseline_receipt(bundle)
    bundle["run_ledger"] = run_ledger(bundle)
    return bundle


def reseal_results(bundle: dict) -> None:
    seal(bundle["current"])
    seal(bundle["baseline"])
    bundle["baseline_receipt"] = baseline_receipt(bundle)
    bundle["run_ledger"] = run_ledger(bundle)


def score(bundle: dict) -> dict:
    return MODULE.evaluate(
        bundle["spec"],
        bundle["public"],
        bundle["corpus"],
        bundle["current"],
        bundle["baseline"],
        bundle["baseline_receipt"],
        bundle["run_ledger"],
        metric_spec_file_digest=bundle["metric_spec_file_digest"],
        runner_file_digest=bundle["runner_digest"],
        current_binary_digest=bundle["current_binary_digest"],
        baseline_binary_digest=bundle["baseline_binary_digest"],
    )


class RetrievalGateTests(unittest.TestCase):
    def test_complete_v2_evidence_can_pass(self):
        report = score(evidence())
        self.assertEqual(report["status"], "PASS")
        self.assertTrue(report["claimable"])
        self.assertEqual(report["task_count"], 220)
        self.assertEqual(report["metrics"]["authorized_action_count"], 200)
        self.assertEqual(
            report["metrics"]["calibration"]["calibrated_task_fraction"], 1.0
        )

    def test_runner_result_builder_is_accepted_by_scorer_index(self):
        bundle = evidence()
        indexed, blockers = MODULE._index_results(
            bundle["current"],
            bundle["public"],
            bundle["corpus"],
            bundle["spec"],
            "current",
            bundle["metric_spec_file_digest"],
            bundle["runner_digest"],
            bundle["current_binary_digest"],
        )
        self.assertEqual(blockers, [])
        self.assertEqual(len(indexed), 220)

    def test_legacy_v1_artifacts_remain_historical(self):
        bundle = evidence()
        bundle["spec"]["schema"] = "m1nd10-g6-metric-spec-v1"
        seal(bundle["spec"])
        bundle["current"]["schema"] = "m1nd10-g6-retrieval-results-v1"
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(any("legacy" in blocker for blocker in report["blockers"]))

    def test_public_and_sealed_v1_corpora_are_explicitly_historical(self):
        bundle = evidence()
        bundle["public"]["schema"] = MODULE.HISTORICAL_PUBLIC_SCHEMA
        bundle["public"]["version"] = 1
        seal(bundle["public"])
        bundle["corpus"]["schema"] = MODULE.HISTORICAL_CASE_SCHEMA
        bundle["corpus"]["version"] = 1
        seal(bundle["corpus"])
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(
            any(
                "held-out-v1 evidence is historical" in blocker
                for blocker in report["blockers"]
            )
        )

    def test_tampered_source_bindings_and_wrong_lane_are_not_proven(self):
        bundle = evidence()
        current = bundle["current"]
        current["corpus_digest"] = digest("wrong-corpus")
        current["source_manifest_digest"] = digest("wrong-manifest")
        current["source_revision"] = "wrong-source"
        current["lane"] = "baseline"
        current["run_metadata"]["lane"] = "baseline"
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        joined = "\n".join(report["blockers"])
        self.assertIn("current corpus_digest", joined)
        self.assertIn("current top-level lane", joined)

    def test_all_execution_markers_removed_is_not_proven(self):
        bundle = evidence()
        for row in bundle["current"]["measurements"]:
            row.pop("north_executed")
            row.pop("seek_executed")
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(
            any("executed seek" in blocker for blocker in report["blockers"])
        )

    def test_eligibility_markers_removed_is_not_proven(self):
        bundle = evidence()
        metadata = bundle["current"]["run_metadata"]
        metadata.pop("score_eligible")
        metadata.pop("diagnostic_only")
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(
            any("score eligibility" in blocker for blocker in report["blockers"])
        )

    def test_delivery1_incomplete_preflights_remain_unscorable(self):
        bundle = evidence()
        metadata = bundle["current"]["run_metadata"]
        metadata["score_eligible"] = False
        metadata["diagnostic_only"] = True
        metadata["proof_state"] = "NOT_PROVEN"
        metadata["formal_preflights"] = {"complete": False}
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(
            any("formal_preflights" in blocker for blocker in report["blockers"])
        )

    def test_declared_eligibility_without_formal_evidence_is_not_proven(self):
        bundle = evidence()
        calibration = copy.deepcopy(bundle["current"]["run_metadata"]["calibration"])
        bundle["current"]["run_metadata"] = {
            "schema": MODULE.RUN_METADATA_SCHEMA,
            "lane": "current",
            "run_id": "run-current",
            "errors": [],
            "actions_executed": 0,
            "labels_read": False,
            "unscored": True,
            "score_eligible": True,
            "diagnostic_only": False,
            "raw_runtime_verdict_counts": {"abstain": 20, "act": 200},
            "calibration": calibration,
        }
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(
            any("formal_preflights" in blocker for blocker in report["blockers"])
        )

    def test_forged_cleanup_summary_is_not_proven(self):
        bundle = evidence()
        bundle["current"]["run_metadata"]["owner_cleanup"][0][
            "process_group_terminated"
        ] = False
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(
            any("cleanup is incomplete" in item for item in report["blockers"])
        )

    def test_foreign_owner_readiness_binary_is_not_proven(self):
        bundle = evidence()
        bundle["current"]["run_metadata"]["owner_topology"][0]["readiness"][
            "binary_digest"
        ] = digest("foreign-owner-binary")
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(
            any("owner readiness binding" in item for item in report["blockers"])
        )

    def test_missing_authority_receipt_proof_is_not_proven(self):
        bundle = evidence()
        bundle["current"]["run_metadata"]["governed_graph_ingest"][0][
            "authority_receipt"
        ]["signature_verified"] = False
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(
            any("production authority proof" in item for item in report["blockers"])
        )

    def test_post_ingest_source_mismatch_is_not_proven(self):
        bundle = evidence()
        bundle["current"]["run_metadata"]["post_ingest_source_verification"][
            "checked_lines"
        ] += 1
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(
            any("post-ingest source" in item for item in report["blockers"])
        )

    def test_absent_blind_boundary_proof_is_not_proven(self):
        bundle = evidence()
        bundle["current"]["run_metadata"]["formal_preflights"].pop(
            "authority_blind_boundary"
        )
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(any("formal_preflights" in item for item in report["blockers"]))

    def test_broken_path_topology_is_not_proven(self):
        bundle = evidence()
        paths = bundle["current"]["run_metadata"]["formal_preflights"]["path_topology"][
            "paths"
        ]
        paths["registry_dir"] = paths["runtime_dir"]
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(
            any("path topology overlaps" in item for item in report["blockers"])
        )

    def test_current_candidate_reused_as_baseline_is_not_proven(self):
        bundle = evidence()
        baseline = bundle["baseline"]
        baseline["system_revision"] = bundle["current"]["system_revision"]
        baseline["binary_digest"] = bundle["current"]["binary_digest"]
        bundle["baseline_binary_digest"] = bundle["current_binary_digest"]
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(
            any(
                "candidate identities are not distinct" in blocker
                for blocker in report["blockers"]
            )
        )

    def test_zero_act_calibration_is_not_proven_not_zero_error_rate(self):
        bundle = evidence()
        current = bundle["current"]
        for row in current["measurements"]:
            if row["verdict"] == "act":
                row["verdict"] = "reverify"
                row["trust_envelope"]["verdict"] = "reverify"
        current["run_metadata"]["raw_runtime_verdict_counts"] = {
            "abstain": 20,
            "reverify": 200,
        }
        current["run_metadata"]["calibration"]["authorized_action_count"] = 0
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(
            any("authorized-action sample" in blocker for blocker in report["blockers"])
        )

    def test_missing_baseline_ratification_receipt_is_not_proven(self):
        bundle = evidence()
        bundle["baseline_receipt"] = {}
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(any("baseline" in blocker for blocker in report["blockers"]))

    def test_duplicate_sealed_run_identity_is_not_proven(self):
        bundle = evidence()
        ledger = bundle["run_ledger"]
        previous = ledger["entries"][-1]["entry_digest"]
        duplicate = copy.deepcopy(ledger["entries"][-1])
        duplicate["sequence"] = 3
        duplicate["previous_entry_digest"] = previous
        duplicate["run_id"] = "second-run-id-same-identity"
        # A new harness/spec does not authorize rerunning the same lane/corpus
        # candidate identity; only a new system revision or binary may do so.
        duplicate["runner_digest"] = digest("changed-runner")
        duplicate["metric_spec_digest"] = digest("changed-metric-spec")
        duplicate["entry_digest"] = MODULE._entry_digest(duplicate)
        ledger["entries"].append(duplicate)
        ledger["entry_count"] = 3
        ledger["final_entry_digest"] = duplicate["entry_digest"]
        seal(ledger)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(
            any(
                "duplicate sealed-run identity" in blocker
                for blocker in report["blockers"]
            )
        )

    def test_runner_metric_and_binary_mismatches_are_not_proven(self):
        bundle = evidence()
        bundle["current"]["runner_digest"] = digest("different-runner")
        bundle["current"]["metric_spec_digest"] = digest("different-spec")
        bundle["current"]["binary_digest"] = digest("different-binary")
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        joined = "\n".join(report["blockers"])
        self.assertIn("current runner_digest", joined)
        self.assertIn("current metric_spec_digest", joined)
        self.assertIn("current binary_digest", joined)

    def test_public_or_sealed_self_digest_tamper_is_not_proven(self):
        bundle = evidence()
        bundle["public"]["runner_contract"]["labels_exposed"] = True
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertTrue(
            any(
                "public corpus self_digest mismatch" in blocker
                for blocker in report["blockers"]
            )
        )

    def test_missing_measurement_is_not_proven(self):
        bundle = evidence()
        bundle["current"]["measurements"].pop()
        bundle["current"]["run_metadata"]["calibration"]["calibrated_task_count"] -= 1
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertFalse(report["claimable"])

    def test_unratified_seek_slo_is_not_proven(self):
        bundle = evidence()
        bundle["spec"]["latency_slo_ms"]["seek_p95"] = None
        seal(bundle["spec"])
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")

    def test_wrong_ground_action_authorization_fails_after_evidence_is_complete(self):
        bundle = evidence()
        current = bundle["current"]
        for row in current["measurements"][200:]:
            row["verdict"] = "act"
            row["trust_envelope"]["verdict"] = "act"
        current["run_metadata"]["raw_runtime_verdict_counts"] = {"act": 220}
        current["run_metadata"]["calibration"]["authorized_action_count"] = 220
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "FAIL")
        self.assertFalse(report["checks"]["wrong_ground_action_rate"])

    def test_error_fallback_latency_is_not_proven(self):
        bundle = evidence()
        current = bundle["current"]
        current["measurements"][0]["seek_latency_ms"] = 0.000001
        current["run_metadata"]["errors"] = [
            {"task_id": "g6-mcp-0000000000000000", "error": "transport failed"}
        ]
        current["run_metadata"]["raw_runtime_verdict_counts"]["error_fallback"] = 1
        reseal_results(bundle)
        report = score(bundle)
        self.assertEqual(report["status"], "NOT_PROVEN")
        self.assertFalse(report["claimable"])


if __name__ == "__main__":
    unittest.main()
