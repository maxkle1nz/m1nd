import importlib.util
import hashlib
import io
import json
import subprocess
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path
from types import SimpleNamespace


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "m1nd10_release_candidate", ROOT / "scripts" / "m1nd10_release_candidate.py"
)
assert SPEC and SPEC.loader
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


COMMIT = "a" * 40
VERSION = "1.4.0"
REF = f"refs/tags/v{VERSION}"
TARGETS = [
    "linux-x86_64",
    "macos-aarch64",
    "macos-x86_64",
    "windows-x86_64",
]
UI_PACKAGE_VERSION = "0.1.0"
UI_FILES = {
    "assets/app.js": b"console.log('sealed candidate UI');\n",
    "index.html": b"<!doctype html><main>M1ND sealed UI</main>\n",
}


def ui_digest() -> str:
    digest = hashlib.sha256(b"m1nd-ui-bundle-tree-v1\0")
    for relative, payload in sorted(UI_FILES.items()):
        encoded = relative.encode("utf-8")
        digest.update(len(encoded).to_bytes(8, "big"))
        digest.update(encoded)
        digest.update(len(payload).to_bytes(8, "big"))
        digest.update(payload)
    return digest.hexdigest()


UI_SHA256 = ui_digest()


class ReleaseCandidateTests(unittest.TestCase):
    def write_crate(
        self,
        artifacts: Path,
        *,
        name: str,
        version: str,
        dependencies: tuple[tuple[str, str], ...] = (),
        dependency_digests: dict[str, str] | None = None,
        dirty: bool = False,
        packaged_ui: bool = False,
    ) -> Path:
        path = artifacts / f"{name}-{version}.crate"
        root = f"{name}-{version}"
        dependency_tables = "\n".join(
            f'[dependencies.{dependency}]\nversion = "{dependency_version}"\n'
            for dependency, dependency_version in dependencies
        )
        manifest = (
            f'[package]\nname = "{name}"\nversion = "{version}"\n'
            'edition = "2021"\nreadme = false\nlicense = "MIT"\n\n'
            f"{dependency_tables}"
        ).encode()
        vcs = json.dumps({"git": {"sha1": COMMIT, "dirty": dirty}}).encode()
        dependency_digests = dependency_digests or {}
        root_dependencies = ""
        if dependencies:
            names = ", ".join(json.dumps(name) for name, _version in dependencies)
            root_dependencies = f"dependencies = [{names}]\n"
        lock = (
            "version = 4\n\n"
            f'[[package]]\nname = "{name}"\nversion = "{version}"\n'
            f"{root_dependencies}"
            + "".join(
                f'\n[[package]]\nname = "{dependency}"\n'
                f'version = "{dependency_version}"\n'
                f'source = "{release.crate_package.CRATES_IO_LOCK_SOURCE}"\n'
                f'checksum = "{dependency_digests[dependency]}"\n'
                for dependency, dependency_version in dependencies
            )
        ).encode()
        with tarfile.open(path, "w:gz") as bundle:
            for member_name, payload in (
                (f"{root}/Cargo.toml", manifest),
                (f"{root}/Cargo.lock", lock),
                (f"{root}/.cargo_vcs_info.json", vcs),
            ):
                info = tarfile.TarInfo(member_name)
                info.mode = 0o644
                info.size = len(payload)
                bundle.addfile(info, io.BytesIO(payload))
            if packaged_ui:
                for relative, payload in UI_FILES.items():
                    info = tarfile.TarInfo(f"{root}/ui-dist/{relative}")
                    info.mode = 0o644
                    info.size = len(payload)
                    bundle.addfile(info, io.BytesIO(payload))
                payload = json.dumps({"version": UI_PACKAGE_VERSION}).encode()
                info = tarfile.TarInfo(f"{root}/ui-package.json")
                info.mode = 0o644
                info.size = len(payload)
                bundle.addfile(info, io.BytesIO(payload))
        return path

    def write_npm_tarball(
        self,
        artifacts: Path,
        *,
        package_name: str = release.NPM_PACKAGE_NAME,
        package_version: str = VERSION,
        member_name: str = "package/package.json",
        publish_config: object | None = None,
    ) -> Path:
        tarball = artifacts / f"maxkle1nz-m1nd-{package_version}.tgz"
        package = {"name": package_name, "version": package_version}
        if publish_config is not None:
            package["publishConfig"] = publish_config
        payload = json.dumps(package, sort_keys=True).encode("utf-8")
        with tarfile.open(tarball, "w:gz") as bundle:
            info = tarfile.TarInfo(member_name)
            info.mode = 0o644
            info.size = len(payload)
            bundle.addfile(info, io.BytesIO(payload))
        return tarball

    def fixture(self, root: Path) -> SimpleNamespace:
        artifacts = root / "artifacts"
        artifacts.mkdir()
        (artifacts / release.UI_BUNDLE_PROVENANCE_NAME).write_text(
            json.dumps(
                {
                    "schema": "m1nd-ui-bundle-provenance-v1",
                    "bundle_sha256": UI_SHA256,
                    "file_count": len(UI_FILES),
                    "node_version": "v22.0.0",
                    "npm_version": "10.0.0",
                    "package_lock_sha256": "c" * 64,
                    "package_version": UI_PACKAGE_VERSION,
                    "placeholder": False,
                    "source_commit": COMMIT,
                }
            )
        )
        self.write_npm_tarball(artifacts)
        core = self.write_crate(artifacts, name="m1nd-core", version=VERSION)
        control = self.write_crate(artifacts, name="m1nd-control", version="0.1.0")
        ingest = self.write_crate(
            artifacts,
            name="m1nd-ingest",
            version=VERSION,
            dependencies=(("m1nd-core", VERSION),),
            dependency_digests={"m1nd-core": hashlib.sha256(core.read_bytes()).hexdigest()},
        )
        self.write_crate(
            artifacts,
            name="m1nd-mcp",
            version=VERSION,
            dependencies=(
                ("m1nd-control", "0.1.0"),
                ("m1nd-core", VERSION),
                ("m1nd-ingest", VERSION),
            ),
            dependency_digests={
                "m1nd-control": hashlib.sha256(control.read_bytes()).hexdigest(),
                "m1nd-core": hashlib.sha256(core.read_bytes()).hexdigest(),
                "m1nd-ingest": hashlib.sha256(ingest.read_bytes()).hexdigest(),
            },
            dirty=True,
            packaged_ui=True,
        )
        for target in TARGETS:
            payload = target.encode()
            digest = hashlib.sha256(payload).hexdigest()
            suffix = "zip" if target.startswith("windows") else "tar.gz"
            archive = artifacts / f"m1nd-mcp-{target}.{suffix}"
            raw = artifacts / release.raw_asset_name(target)
            raw.write_bytes(payload)
            member = release.expected_archive_member(target)
            if target.startswith("windows"):
                with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
                    bundle.writestr(member, payload)
            else:
                with tarfile.open(archive, "w:gz") as bundle:
                    info = tarfile.TarInfo(member)
                    info.mode = 0o755
                    info.size = len(payload)
                    bundle.addfile(info, io.BytesIO(payload))
            (artifacts / f"GATE-ARTIFACT-SMOKE-{target}.json").write_text(
                json.dumps(
                    {
                        "binary": {
                            "sha256": digest,
                            "version_output": f"m1nd-mcp {VERSION} ({COMMIT[:9]})",
                        },
                        "expected": {"commit": COMMIT, "version": VERSION},
                        "schema": "m1nd-release-artifact-smoke-v1",
                        "target": target,
                        "ui_bundle": {
                            "freshness": "FRESH",
                            "mode": "embedded",
                            "sha256": UI_SHA256,
                            "status": "AVAILABLE",
                        },
                        "verdict": "PASS",
                    }
                )
            )
        (artifacts / release.SBOM_NAME).write_text('{"spdxVersion":"SPDX-2.3"}')
        return SimpleNamespace(
            artifacts=artifacts,
            version=VERSION,
            commit=COMMIT,
            source_ref=REF,
            run_id="42",
            expected_target=TARGETS,
            required_job=["rust-gates", "ui-gates"],
            output=artifacts / "CANDIDATE.json",
            receipt_output=artifacts / "GATE-RECEIPT.json",
            rollback_output=artifacts / "ROLLBACK.json",
        )

    def test_assemble_and_verify_bind_exact_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            args = self.fixture(root)
            release.assemble(args)
            (args.artifacts / "SHA256SUMS").write_text("sealed later in the workflow\n")
            (args.artifacts / "CANDIDATE.json.sigstore.json").write_text("{}\n")
            release.verify(SimpleNamespace(artifacts=args.artifacts, manifest=args.output))
            manifest = json.loads(args.output.read_text())
            self.assertEqual(manifest["schema"], release.SCHEMA)
            self.assertEqual(manifest["build_policy"]["builds_per_target"], 1)
            self.assertEqual(manifest["build_policy"]["cargo_packages_per_crate"], 1)
            self.assertTrue(manifest["build_policy"]["raw_asset_install"])
            self.assertEqual(len(manifest["runtime_bindings"]), len(TARGETS))
            self.assertEqual(manifest["npm_package"]["kind"], "npm_package_tarball")
            self.assertEqual(
                manifest["npm_package"]["package_name"], release.NPM_PACKAGE_NAME
            )
            self.assertEqual(manifest["npm_package"]["package_version"], VERSION)
            self.assertEqual(
                [package["package_name"] for package in manifest["cargo_packages"]],
                ["m1nd-core", "m1nd-control", "m1nd-ingest", "m1nd-mcp"],
            )
            self.assertEqual(
                [package["publish_order"] for package in manifest["cargo_packages"]],
                [1, 2, 3, 4],
            )
            self.assertEqual(
                manifest["cargo_packages"][-1]["ui_bundle_sha256"], UI_SHA256
            )
            self.assertEqual(
                manifest["npm_package"],
                next(
                    entry
                    for entry in manifest["artifacts"]
                    if entry["kind"] == "npm_package_tarball"
                ),
            )
            self.assertEqual(
                next(
                    entry["ui_bundle_sha256"]
                    for entry in manifest["artifacts"]
                    if entry["kind"] == "ui_bundle_provenance"
                ),
                UI_SHA256,
            )
            self.assertTrue(
                all(
                    binding["runtime_sha256"]
                    == next(
                        entry["sha256"]
                        for entry in manifest["artifacts"]
                        if entry["kind"] == "runtime_binary"
                        and entry["target"] == binding["target"]
                    )
                    for binding in manifest["runtime_bindings"]
                )
            )
            self.assertEqual(json.loads(args.receipt_output.read_text())["decision"], "PASS")
            self.assertFalse(
                json.loads(args.rollback_output.read_text())["rollback"]["automatic"]
            )

    def test_verify_refuses_changed_artifact(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            args = self.fixture(root)
            release.assemble(args)
            first = next(args.artifacts.glob("*.tar.gz"))
            first.write_bytes(first.read_bytes() + b"changed")
            with self.assertRaises(release.CandidateError):
                release.verify(SimpleNamespace(artifacts=args.artifacts, manifest=args.output))

    def test_assemble_refuses_missing_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            next(args.artifacts.glob("*windows*")).unlink()
            with self.assertRaises(release.CandidateError):
                release.build_documents(args)

    def test_assemble_refuses_missing_or_failed_smoke_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            receipt = next(args.artifacts.glob("GATE-ARTIFACT-SMOKE-*"))
            receipt.unlink()
            with self.assertRaises(release.CandidateError):
                release.build_documents(args)

    def test_assemble_refuses_missing_or_mismatched_ui_provenance(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            (args.artifacts / release.UI_BUNDLE_PROVENANCE_NAME).unlink()
            with self.assertRaisesRegex(release.CandidateError, "UI-BUNDLE-PROVENANCE"):
                release.build_documents(args)

        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            receipt = next(args.artifacts.glob("GATE-ARTIFACT-SMOKE-*"))
            value = json.loads(receipt.read_text())
            value["ui_bundle"]["sha256"] = "d" * 64
            receipt.write_text(json.dumps(value))
            with self.assertRaisesRegex(release.CandidateError, "UI identity"):
                release.build_documents(args)

    def test_assemble_refuses_missing_or_wrong_version_npm_tarball(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            next(args.artifacts.glob("*.tgz")).unlink()
            with self.assertRaisesRegex(release.CandidateError, "npm package tarball"):
                release.build_documents(args)

        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            next(args.artifacts.glob("*.tgz")).unlink()
            self.write_npm_tarball(args.artifacts, package_version="1.4.1")
            with self.assertRaisesRegex(release.CandidateError, "candidate version"):
                release.build_documents(args)

    def test_assemble_refuses_missing_crate_or_mcp_ui_drift(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            (args.artifacts / f"m1nd-control-0.1.0.crate").unlink()
            with self.assertRaisesRegex(release.CandidateError, "Cargo package set mismatch"):
                release.build_documents(args)

        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            (args.artifacts / f"m1nd-mcp-{VERSION}.crate").unlink()
            self.write_crate(
                args.artifacts,
                name="m1nd-mcp",
                version=VERSION,
                dependencies=(
                    ("m1nd-control", "0.1.0"),
                    ("m1nd-core", VERSION),
                    ("m1nd-ingest", VERSION),
                ),
                dependency_digests={
                    "m1nd-control": hashlib.sha256(
                        (args.artifacts / "m1nd-control-0.1.0.crate").read_bytes()
                    ).hexdigest(),
                    "m1nd-core": hashlib.sha256(
                        (args.artifacts / f"m1nd-core-{VERSION}.crate").read_bytes()
                    ).hexdigest(),
                    "m1nd-ingest": hashlib.sha256(
                        (args.artifacts / f"m1nd-ingest-{VERSION}.crate").read_bytes()
                    ).hexdigest(),
                },
                dirty=True,
                packaged_ui=False,
            )
            with self.assertRaisesRegex(release.CandidateError, "exact sealed UI"):
                release.build_documents(args)

        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            mcp = args.artifacts / f"m1nd-mcp-{VERSION}.crate"
            mcp.unlink()
            self.write_crate(
                args.artifacts,
                name="m1nd-mcp",
                version=VERSION,
                dependencies=(
                    ("m1nd-control", "0.1.0"),
                    ("m1nd-core", VERSION),
                    ("m1nd-ingest", VERSION),
                ),
                dependency_digests={
                    "m1nd-control": "f" * 64,
                    "m1nd-core": "f" * 64,
                    "m1nd-ingest": "f" * 64,
                },
                dirty=True,
                packaged_ui=True,
            )
            with self.assertRaisesRegex(release.CandidateError, "lock bytes"):
                release.build_documents(args)

    def test_npm_tarball_inspection_refuses_identity_and_path_confusion(self):
        with tempfile.TemporaryDirectory() as temporary:
            artifacts = Path(temporary)
            wrong_name = self.write_npm_tarball(
                artifacts, package_name="@attacker/not-m1nd"
            )
            with self.assertRaisesRegex(release.CandidateError, "canonical"):
                release.inspect_npm_package(wrong_name)

        with tempfile.TemporaryDirectory() as temporary:
            artifacts = Path(temporary)
            unsafe = self.write_npm_tarball(
                artifacts, member_name="../package/package.json"
            )
            with self.assertRaisesRegex(release.CandidateError, "unsafe member path"):
                release.inspect_npm_package(unsafe)

    def test_npm_tarball_inspection_refuses_registry_redirects(self):
        with tempfile.TemporaryDirectory() as temporary:
            artifacts = Path(temporary)
            redirected = self.write_npm_tarball(
                artifacts,
                publish_config={"registry": "https://registry.attacker.invalid"},
            )
            with self.assertRaisesRegex(release.CandidateError, "publishConfig.registry"):
                release.inspect_npm_package(redirected)

        with tempfile.TemporaryDirectory() as temporary:
            artifacts = Path(temporary)
            scoped = self.write_npm_tarball(
                artifacts,
                publish_config={"@maxkle1nz:registry": "https://registry.attacker.invalid"},
            )
            with self.assertRaisesRegex(release.CandidateError, "scoped registry redirect"):
                release.inspect_npm_package(scoped)

        with tempfile.TemporaryDirectory() as temporary:
            artifacts = Path(temporary)
            transport_override = self.write_npm_tarball(
                artifacts,
                publish_config={
                    "access": "public",
                    "https-proxy": "https://registry.attacker.invalid",
                    "strict-ssl": False,
                },
            )
            with self.assertRaisesRegex(release.CandidateError, "unsupported keys"):
                release.inspect_npm_package(transport_override)

        with tempfile.TemporaryDirectory() as temporary:
            artifacts = Path(temporary)
            restricted = self.write_npm_tarball(
                artifacts,
                publish_config={"access": "restricted"},
            )
            with self.assertRaisesRegex(release.CandidateError, "access must be exactly public"):
                release.inspect_npm_package(restricted)

        with tempfile.TemporaryDirectory() as temporary:
            artifacts = Path(temporary)
            canonical = self.write_npm_tarball(
                artifacts,
                publish_config={"registry": release.NPM_REGISTRY, "access": "public"},
            )
            self.assertEqual(
                release.inspect_npm_package(canonical),
                (release.NPM_PACKAGE_NAME, VERSION),
            )

    def test_assemble_refuses_archive_raw_digest_mismatch(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            raw = args.artifacts / release.raw_asset_name("linux-x86_64")
            raw.write_bytes(raw.read_bytes() + b"changed")
            with self.assertRaisesRegex(release.CandidateError, "archive/raw runtime mismatch"):
                release.build_documents(args)

    def test_assemble_refuses_self_referential_pre_signature_update_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            receipt_path = args.artifacts / "GATE-VERIFIED-UPDATE-SMOKE-macos-aarch64.json"
            receipt_path.write_text("{}\n")
            with self.assertRaisesRegex(
                release.CandidateError, "unrecognized release artifact refused"
            ):
                release.build_documents(args)

    def test_assemble_refuses_unrecognized_release_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = self.fixture(Path(temporary))
            (args.artifacts / "unbound-payload.bin").write_bytes(b"not declared")
            with self.assertRaisesRegex(
                release.CandidateError, "unrecognized release artifact refused"
            ):
                release.build_documents(args)

    def test_raw_asset_names_are_updater_compatible(self):
        self.assertEqual(release.raw_asset_name("linux-x86_64"), "m1nd-mcp-linux-x86_64")
        self.assertEqual(
            release.raw_asset_name("windows-x86_64"),
            "m1nd-mcp-windows-x86_64.exe",
        )

    def test_identity_requires_exact_version_tag_and_full_commit(self):
        with self.assertRaises(release.CandidateError):
            release.validate_identity(VERSION, COMMIT, "refs/heads/main")
        with self.assertRaises(release.CandidateError):
            release.validate_identity(VERSION, "abc123", REF)

    def test_python_and_node_canonical_json_match(self):
        value = {
            "z": [{"beta": 2, "alpha": 1}],
            "a": {"unicode": "m1nd", "nested": [3, 2, 1]},
        }
        script = (
            "const fs=require('fs');"
            "const {canonicalJson}=require('./npm/lib/cli');"
            "process.stdout.write(canonicalJson(JSON.parse(fs.readFileSync(0,'utf8'))));"
        )
        result = subprocess.run(
            ["node", "-e", script],
            cwd=ROOT,
            input=json.dumps(value),
            text=True,
            capture_output=True,
            check=True,
        )
        self.assertEqual(result.stdout.encode(), release.canonical_json(value))

    def test_verify_post_signature_update_receipts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            args = self.fixture(root)
            release.assemble(args)
            manifest = json.loads(args.output.read_text())
            manifest_sha256 = release.sha256_file(args.output)
            receipts = root / "verified-update-receipts"
            receipts.mkdir()
            identity = (
                "https://github.com/maxkle1nz/m1nd/.github/workflows/"
                f"release.yml@refs/tags/v{VERSION}"
            )
            for binding in manifest["runtime_bindings"]:
                target = binding["target"]
                receipt = {
                    "schema": "m1nd-release-verified-update-smoke-v2",
                    "target": target,
                    "candidate_id": manifest["candidate_id"],
                    "candidate_manifest_sha256": manifest_sha256,
                    "candidate_verification": {
                        "candidate_id": manifest["candidate_id"],
                        "manifest_sha256": manifest_sha256,
                        "target": target,
                        "raw_sha256": binding["runtime_sha256"],
                        "raw_size_bytes": binding["size_bytes"],
                        "certificate_identity": identity,
                        "certificate_oidc_issuer": "https://token.actions.githubusercontent.com",
                        "verifier_source": "trusted-fixed-path",
                        "transport_source": "local-test-directory",
                    },
                    "test_overrides": {
                        "active": True,
                        "release_transport": "local-test-directory",
                        "verifier_source": "trusted-fixed-path",
                    },
                    "proofs": {
                        proof: True for proof in release.VERIFIED_UPDATE_PROOFS
                    },
                    "verdict": "PASS",
                }
                (receipts / f"GATE-VERIFIED-UPDATE-SMOKE-{target}.json").write_text(
                    json.dumps(receipt)
                )
            verify_args = SimpleNamespace(receipts=receipts, manifest=args.output)
            release.verify_update_receipts(verify_args)
            first = next(receipts.iterdir())
            tampered = json.loads(first.read_text())
            tampered["candidate_verification"]["raw_sha256"] = "f" * 64
            first.write_text(json.dumps(tampered))
            with self.assertRaisesRegex(
                release.CandidateError, "not bound to the signed candidate"
            ):
                release.verify_update_receipts(verify_args)


if __name__ == "__main__":
    unittest.main()
