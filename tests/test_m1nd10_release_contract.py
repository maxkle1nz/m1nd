import copy
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace


ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
sys.path.insert(0, str(SCRIPTS))
import m1nd10_release_contract as contract  # noqa: E402
from tests import test_m1nd10_release_candidate as candidate_fixtures  # noqa: E402

SPEC = importlib.util.spec_from_file_location(
    "m1nd10_release_candidate_canonical", SCRIPTS / "m1nd10_release_candidate.py"
)
assert SPEC and SPEC.loader
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)

VECTORS = ROOT / "tests" / "fixtures" / "M1ND10-CANONICAL-VECTORS.json"


class CanonicalReleaseContractTests(unittest.TestCase):
    def vectors(self):
        return contract.load_integer_json(VECTORS, "test vectors")

    def test_checked_in_vectors_cover_exact_candidate_g0_g10_and_review(self):
        vectors = contract.verify_vectors(self.vectors())
        evidence = vectors["evidence_set"]
        self.assertEqual(
            [receipt["core"]["gate_id"] for receipt in evidence["gate_receipts"]],
            list(contract.GATE_IDS),
        )
        self.assertEqual(
            evidence["contract_status"],
            contract.STRUCTURAL_STATUS,
        )
        self.assertTrue(
            all(
                receipt["signature"].startswith(contract.FIXTURE_SIGNATURE_PREFIX)
                for receipt in evidence["gate_receipts"]
            )
        )

    def test_canonical_json_is_utf8_newline_free_and_lossless_for_u64(self):
        value = {"z": "coração", "built_at": 9007199254740993, "a": "α"}
        encoded = contract.canonical_json(value)
        self.assertEqual(
            encoded,
            '{"a":"α","built_at":9007199254740993,"z":"coração"}'.encode(),
        )
        self.assertFalse(encoded.endswith(b"\n"))
        self.assertEqual(contract.loads_integer_json(encoded.decode()), value)

    def test_every_float_representation_is_refused(self):
        for text in ('{"n":1.0}', '{"n":1e30}', '{"n":NaN}'):
            with (
                self.subTest(text=text),
                self.assertRaises(contract.ReleaseContractError),
            ):
                contract.loads_integer_json(text)
        with self.assertRaises(contract.ReleaseContractError):
            contract.canonical_json({"n": 1.0})

    def test_duplicate_json_object_members_are_refused_at_every_depth(self):
        for text in ('{"a":1,"a":2}', '{"outer":{"a":1,"a":2}}'):
            with (
                self.subTest(text=text),
                self.assertRaisesRegex(
                    contract.ReleaseContractError, "duplicate JSON object key"
                ),
            ):
                contract.loads_integer_json(text)

    def test_validator_matches_rust_nonempty_signature_law(self):
        candidate = copy.deepcopy(self.vectors()["evidence_set"]["candidate"])
        candidate["provenance_signature"] = " "
        contract.validate_candidate(candidate)
        candidate["provenance_signature"] = ""
        with self.assertRaises(contract.ReleaseContractError):
            contract.validate_candidate(candidate)

    def test_finding_info_wire_value_is_not_screaming_snake_case(self):
        receipt = copy.deepcopy(self.vectors()["evidence_set"]["gate_receipts"][0])
        receipt["core"]["findings"][0]["severity"] = "INFO"
        with self.assertRaisesRegex(contract.ReleaseContractError, "severity"):
            contract.validate_gate_receipt(receipt)

    def test_candidate_and_receipt_unknown_fields_are_refused(self):
        candidate = copy.deepcopy(self.vectors()["evidence_set"]["candidate"])
        candidate["future"] = True
        with self.assertRaisesRegex(contract.ReleaseContractError, "unknown"):
            contract.validate_candidate(candidate)
        receipt = copy.deepcopy(self.vectors()["evidence_set"]["gate_receipts"][0])
        receipt["core"]["future"] = True
        with self.assertRaisesRegex(contract.ReleaseContractError, "unknown"):
            contract.validate_gate_receipt(receipt)

    def test_convergence_refuses_missing_duplicate_drift_and_open_p1(self):
        evidence = copy.deepcopy(self.vectors()["evidence_set"])
        evidence["gate_receipts"].pop()
        with self.assertRaisesRegex(contract.ReleaseContractError, "missing gates"):
            contract.validate_convergence(evidence)

        evidence = copy.deepcopy(self.vectors()["evidence_set"])
        evidence["gate_receipts"][-1] = copy.deepcopy(evidence["gate_receipts"][0])
        with self.assertRaisesRegex(contract.ReleaseContractError, "duplicate gate"):
            contract.validate_convergence(evidence)

        evidence = copy.deepcopy(self.vectors()["evidence_set"])
        review_core = evidence["independent_review"]["core"]
        review_core["candidate_digest"] = "f" * 64
        evidence["independent_review"] = contract.seal_independent_review(
            review_core, "NOT_CRYPTOGRAPHIC:fixture-drift"
        )
        with self.assertRaisesRegex(
            contract.ReleaseContractError, "candidate mismatch"
        ):
            contract.validate_convergence(evidence)

        evidence = copy.deepcopy(self.vectors()["evidence_set"])
        review_core = evidence["independent_review"]["core"]
        review_core["findings"] = [
            {
                "finding_id": "blocking",
                "severity": "P1",
                "status": "OPEN",
                "statement": "fixture blocker",
                "evidence_digest": "a" * 64,
            }
        ]
        evidence["independent_review"] = contract.seal_independent_review(
            review_core, "NOT_CRYPTOGRAPHIC:fixture-blocker"
        )
        with self.assertRaisesRegex(contract.ReleaseContractError, "open P0/P1"):
            contract.validate_convergence(evidence)

    def test_builder_marker_policy_is_stricter_than_validator_only_at_emission(self):
        release._require_builder_signature("NOT_CRYPTOGRAPHIC:fixture", True)
        release._require_builder_signature("opaque-production-reference", False)
        with self.assertRaises(release.CandidateError):
            release._require_builder_signature("opaque-unmarked-fixture", True)
        with self.assertRaises(release.CandidateError):
            release._require_builder_signature(
                "NOT_CRYPTOGRAPHIC:not-production", False
            )

    def test_g8_pass_builder_requires_full_g8_surface(self):
        receipt = copy.deepcopy(self.vectors()["evidence_set"]["gate_receipts"][8])
        receipt["core"]["artifact_digests"] = {"updater_smoke": "a" * 64}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            core = root / "core.json"
            core.write_text(json.dumps(receipt["core"]), encoding="utf-8")
            with self.assertRaisesRegex(release.CandidateError, "updater smokes alone"):
                release.seal_canonical_gate(
                    SimpleNamespace(
                        core=core,
                        signature="NOT_CRYPTOGRAPHIC:fixture-G8",
                        fixture_only=True,
                        output=root / "G8.json",
                    )
                )

    def test_checked_in_gate_receipts_carry_the_ratified_custody_floor(self):
        vectors = self.vectors()
        for receipt in vectors["evidence_set"]["gate_receipts"]:
            self.assertEqual(
                receipt["core"]["custody_floor"],
                contract.SECURE_ENCLAVE_CUSTODY_FLOOR,
            )

    def test_custody_floor_outside_closed_set_is_refused(self):
        receipt = copy.deepcopy(self.vectors()["evidence_set"]["gate_receipts"][0])
        receipt["core"]["custody_floor"] = "software"
        # Direct validator refusal (before any digest is entertained).
        with self.assertRaisesRegex(contract.ReleaseContractError, "custody_floor"):
            contract.validate_gate_receipt(receipt)
        # Builder refusal: smuggling "software" through the seal-canonical-gate
        # core-input must fail, so the closed set holds at emission time too.
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            core = root / "core.json"
            core.write_text(json.dumps(receipt["core"]), encoding="utf-8")
            with self.assertRaisesRegex(contract.ReleaseContractError, "custody_floor"):
                release.seal_canonical_gate(
                    SimpleNamespace(
                        core=core,
                        signature="NOT_CRYPTOGRAPHIC:fixture-smuggle",
                        fixture_only=True,
                        output=root / "G0.json",
                    )
                )

    def test_fixture_builders_seal_candidate_gate_review_and_evidence(self):
        vectors = self.vectors()
        evidence = vectors["evidence_set"]
        operational = vectors["operational_manifests"]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            compatibility = root / "RELEASE-COMPATIBILITY.json"
            rollback = root / "M1ND10-ROLLBACK.json"
            compatibility.write_bytes(
                contract.canonical_json(operational["compatibility"])
            )
            rollback.write_bytes(contract.canonical_json(operational["rollback"]))
            core_input = root / "core-input.json"
            core_input.write_text(
                json.dumps(
                    {
                        "schema": release.CANONICAL_CORE_INPUT_SCHEMA,
                        "authority": {
                            "authority_class": "FIXTURE_ONLY",
                            "producer_id": "fixture-author",
                            "producer_key_version": "fixture-key-v1",  # gitleaks:allow
                            "authority_receipt_digest": "a" * 64,
                        },
                        "core": evidence["candidate"]["core"],
                    },
                    ensure_ascii=False,
                ),
                encoding="utf-8",
            )
            candidate_path = root / "CANDIDATE.json"
            release.seal_canonical_candidate(
                SimpleNamespace(
                    core_input=core_input,
                    compatibility_manifest=compatibility,
                    rollback_plan=rollback,
                    provenance_signature="NOT_CRYPTOGRAPHIC:fixture-candidate",
                    fixture_only=True,
                    output=candidate_path,
                )
            )
            self.assertEqual(
                contract.load_integer_json(candidate_path, "candidate"),
                evidence["candidate"],
            )
            self.assertFalse(candidate_path.read_bytes().endswith(b"\n"))

            receipts = root / "receipts"
            receipts.mkdir()
            for receipt in evidence["gate_receipts"]:
                gate = receipt["core"]["gate_id"]
                core_path = root / f"{gate}-core.json"
                core_path.write_text(
                    json.dumps(receipt["core"], ensure_ascii=False), encoding="utf-8"
                )
                release.seal_canonical_gate(
                    SimpleNamespace(
                        core=core_path,
                        signature=f"NOT_CRYPTOGRAPHIC:fixture-{gate}",
                        fixture_only=True,
                        output=receipts / f"{gate}.json",
                    )
                )
            review_core = root / "review-core.json"
            review_core.write_text(
                json.dumps(evidence["independent_review"]["core"], ensure_ascii=False),
                encoding="utf-8",
            )
            review_path = root / "IAR.json"
            release.seal_canonical_review(
                SimpleNamespace(
                    core=review_core,
                    signature="NOT_CRYPTOGRAPHIC:fixture-IAR",
                    fixture_only=True,
                    output=review_path,
                )
            )
            output = root / "M1ND10-EVIDENCE-SET.json"
            release.verify_canonical_evidence(
                SimpleNamespace(
                    candidate=candidate_path,
                    receipts=receipts,
                    review=review_path,
                    output=output,
                )
            )
            contract.validate_convergence(
                contract.load_integer_json(output, "evidence extension")
            )

    def test_prepare_operational_manifests_is_non_circular_and_pins_keys(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            release_fixture = candidate_fixtures.ReleaseCandidateTests().fixture(root)
            artifacts = release_fixture.artifacts
            args = SimpleNamespace(
                artifacts=artifacts,
                version=release_fixture.version,
                commit=release_fixture.commit,
                source_ref=release_fixture.source_ref,
                expected_target=release_fixture.expected_target,
                compatibility_output=artifacts / "RELEASE-COMPATIBILITY.json",
                rollback_output=artifacts / "ROLLBACK-CANONICAL.json",
                digest_output=artifacts / "CANONICAL-OPERATIONAL-DIGESTS.json",
            )
            release.prepare_canonical_operational(args)
            descriptor = contract.load_integer_json(args.digest_output, "descriptor")
            self.assertEqual(
                descriptor["artifact_digests"][contract.COMPATIBILITY_ARTIFACT_KEY],
                release.sha256_file(args.compatibility_output),
            )
            self.assertEqual(
                descriptor["artifact_digests"][contract.ROLLBACK_ARTIFACT_KEY],
                release.sha256_file(args.rollback_output),
            )
            self.assertEqual(
                descriptor["artifact_digests"][
                    f"{contract.RELEASE_ARTIFACT_PREFIX}{release.SBOM_NAME}"
                ],
                release.sha256_file(artifacts / release.SBOM_NAME),
            )
            self.assertNotIn("candidate_digest", args.rollback_output.read_text())
            self.assertFalse(args.compatibility_output.read_bytes().endswith(b"\n"))

            tampered_rollback = contract.load_integer_json(
                args.rollback_output, "rollback fixture"
            )
            tampered_rollback["runtime_bindings"][0]["runtime_sha256"] = "f" * 64
            with self.assertRaisesRegex(release.CandidateError, "runtime bytes differ"):
                release.validate_canonical_operational_pair(
                    contract.load_integer_json(
                        args.compatibility_output, "compatibility fixture"
                    ),
                    release.validate_canonical_rollback(tampered_rollback),
                )

            args.compatibility_output.unlink()
            args.rollback_output.unlink()
            args.digest_output.unlink()
            (artifacts / release.SBOM_NAME).unlink()
            with self.assertRaisesRegex(release.CandidateError, "exactly one"):
                release.prepare_canonical_operational(args)


if __name__ == "__main__":
    unittest.main()
