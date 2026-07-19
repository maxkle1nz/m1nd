import base64
import hashlib
import io
import json
import os
import re
import tempfile
import unittest
import urllib.request
from contextlib import redirect_stdout
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SHA_PIN = re.compile(r"@[0-9a-f]{40}(?:\s|$)")


class CiSecurityContractTests(unittest.TestCase):
    def text(self, relative: str) -> str:
        return (ROOT / relative).read_text(encoding="utf-8")

    def assert_immutable_uses(self, workflow: str) -> None:
        for line in self.text(workflow).splitlines():
            if "uses:" in line:
                self.assertRegex(line, SHA_PIN, f"mutable action reference: {line}")

    def job(self, workflow: str, name: str) -> str:
        text = self.text(workflow)
        marker = f"\n  {name}:\n"
        self.assertIn(marker, text, f"missing workflow job: {name}")
        tail = text.split(marker, 1)[1]
        next_job = re.search(r"(?m)^  [a-z0-9-]+:\n", tail)
        return tail[: next_job.start()] if next_job else tail

    def inline_python_after(self, workflow: str, marker: str) -> str:
        lines = self.text(workflow).splitlines()
        marker_index = next(
            (index for index, line in enumerate(lines) if marker in line), None
        )
        self.assertIsNotNone(marker_index, f"missing inline Python marker: {marker}")
        heredoc_index = next(
            (
                index
                for index in range(marker_index + 1, len(lines))
                if "python3 - <<'PY'" in lines[index]
            ),
            None,
        )
        self.assertIsNotNone(heredoc_index, f"missing Python heredoc after: {marker}")
        body = []
        index = heredoc_index + 1
        while index < len(lines) and lines[index].strip() != "PY":
            body.append(lines[index])
            index += 1
        self.assertLess(
            index, len(lines), f"unterminated Python heredoc after: {marker}"
        )
        margins = [len(line) - len(line.lstrip()) for line in body if line.strip()]
        margin = min(margins, default=0)
        return "\n".join(line[margin:] for line in body) + "\n"

    def test_release_authority_is_public_tokenless_and_precedes_privileged_jobs(self):
        release = self.text(".github/workflows/release.yml")
        self.assert_immutable_uses(".github/workflows/release.yml")
        self.assertIn("fetch-depth: 0", release)
        self.assertIn("+refs/heads/main:refs/remotes/origin/main", release)
        self.assertIn('${GITHUB_SHA}" != "${MAIN_HEAD}', release)
        self.assertIn(
            "https://api.github.com/repos/{repository}/releases/tags/", release
        )
        self.assertIn(
            "RELEASE_REPOSITORY_PRIVATE: ${{ github.event.repository.private }}",
            release,
        )
        self.assertIn('repository_private != "false"', release)
        self.assertIn("valid only for a public repository", release)
        self.assertIn("https://registry.npmjs.org/{npm_identity}/", release)
        self.assertIn("https://crates.io/api/v1/crates/", release)
        self.assertIn("exact checksum recovery", release)
        self.assertNotIn("m1nd10_release_authority.py", release)
        self.assertNotIn("${{ github.token }}", release)
        self.assertNotIn("GH_TOKEN", release)
        self.assertNotIn("--github-token", release)
        self.assertGreaterEqual(release.count("environment: release"), 3)

    def test_candidate_source_and_secret_boundaries_are_mandatory(self):
        action = (
            "gitleaks/gitleaks-action@ff98106e4c7b2bc287b24eaf42907196329070c7 # v2.3.9"
        )
        guard = "scripts/m1nd10_candidate_source_guard.py"
        for workflow in (".github/workflows/ci.yml", ".github/workflows/release.yml"):
            text = self.text(workflow)
            self.assertIn(guard, text)
            self.assertIn(action, text)
            self.assertIn("GITLEAKS_VERSION: 8.30.1", text)
            self.assertIn('GITLEAKS_NOTIFY_USER_LIST: ""', text)
            self.assert_immutable_uses(workflow)

        security = self.job(".github/workflows/ci.yml", "security-gates")
        tag_guard = self.job(".github/workflows/release.yml", "tag-guard")
        self.assertIn("fetch-depth: 0", security)
        self.assertIn('--revision "${GITHUB_SHA}"', security)
        self.assertIn("fetch-depth: 0", tag_guard)
        self.assertIn('--revision "${GITHUB_SHA}"', tag_guard)

        policy = self.text("scripts/m1nd10_candidate_source_guard.py")
        self.assertIn(
            'PRIVATE_COMPONENTS = frozenset({"operator-only", "runner-results"})',
            policy,
        )
        self.assertIn('"scripts/benchmark/m1nd10_g6_corpus.py"', policy)
        self.assertIn('"tests/test_m1nd10_g6_held_out_v2_corpus.py"', policy)

    def test_release_builds_only_from_the_sealed_ui_artifact(self):
        release = self.text(".github/workflows/release.yml")
        self.assertIn("ui-artifact:", release)
        self.assertIn("m1nd10_ui_bundle.py create", release)
        self.assertIn("M1ND_RELEASE_UI_REQUIRED", release)
        self.assertIn("M1ND_EXPECTED_UI_BUNDLE_SHA256", release)
        self.assertIn("--expected-ui-sha256", release)
        self.assertIn("UI-BUNDLE-PROVENANCE.json", release)
        build = self.text("m1nd-mcp/build.rs")
        self.assertIn("placeholder UI artifact is forbidden", build)
        self.assertIn("UI digest mismatch", build)

    def test_npm_package_is_packed_once_and_promoted_as_candidate_bytes(self):
        workflow = self.text(".github/workflows/release.yml")
        candidate = self.text("scripts/m1nd10_release_candidate.py")
        self.assertIn("npm-artifact:", workflow)
        self.assertIn("npm pack --json --pack-destination release-npm", workflow)
        self.assertEqual(len(re.findall(r"(?m)^\s*npm pack\b", workflow)), 1)
        self.assertIn("npm_package_tarball", candidate)
        self.assertIn('"npm_package": npm_packages[0]', candidate)
        verification = self.job(".github/workflows/release.yml", "release-verification")
        publish = self.job(".github/workflows/release.yml", "publish-npm")
        self.assertNotRegex(publish, r"(?m)^\s*npm pack\b")
        self.assertIn("m1nd10_release_candidate.py verify", verification)
        self.assertNotIn("m1nd10_release_candidate.py", publish)
        self.assertNotIn("actions/checkout", publish)
        self.assertIn("needs.release-verification.outputs.npm_tarball", publish)
        self.assertIn(
            'npm publish "release-bins/${EXPECTED_NPM_TARBALL}"',
            publish,
        )
        self.assertIn("--ignore-scripts", publish)
        self.assertIn('NPM_CONFIG_REGISTRY: "https://registry.npmjs.org"', publish)
        self.assertIn('--registry "https://registry.npmjs.org"', publish)
        self.assertNotIn("contents: read", publish)
        self.assertIn('NPM_REGISTRY = "https://registry.npmjs.org"', candidate)
        self.assertIn("publishConfig.registry", candidate)
        self.assertIn("scoped registry redirect", candidate)
        self.assertIn('allowed_publish_config = {"access", "registry"}', candidate)
        self.assertIn("publishConfig contains unsupported keys", candidate)
        self.assertIn("publishConfig.access must be exactly public", candidate)
        self.assertIn('expected_integrity = "sha512-"', publish)
        self.assertIn('dist.get("integrity")', publish)
        self.assertIn("observed_integrity != expected_integrity", publish)
        self.assertIn("already published exact signed candidate npm tarball", publish)
        self.assertIn("recovered exact signed candidate npm publication", publish)
        self.assertIn("foreign immutable npm tarball exists", publish)
        self.assertIn("urllib.request.ProxyHandler({})", publish)
        self.assertNotIn("npm view", publish)

    def test_npm_exact_existing_probe_is_behaviorally_integrity_bound(self):
        source = self.inline_python_after(
            ".github/workflows/release.yml", "probe_registry_integrity()"
        )
        compiled = compile(source, "release.yml:npm-integrity-probe", "exec")
        version = "1.4.0"
        filename = "maxkle1nz-m1nd-1.4.0.tgz"
        tarball_bytes = b"signed candidate npm tarball bytes"
        integrity = "sha512-" + base64.b64encode(
            hashlib.sha512(tarball_bytes).digest()
        ).decode("ascii")

        class Response:
            status = 200

            def __init__(self, payload: dict):
                self.payload = json.dumps(payload).encode("utf-8")

            def __enter__(self):
                return self

            def __exit__(self, exc_type, exc_value, traceback):
                return False

            def read(self, _limit: int) -> bytes:
                return self.payload

        class Opener:
            def __init__(self, payload: dict):
                self.payload = payload

            def open(self, request, timeout: int):
                self_request_url = (
                    "https://registry.npmjs.org/%40maxkle1nz%2Fm1nd/1.4.0"
                )
                if request.full_url != self_request_url or timeout != 20:
                    raise AssertionError(f"unexpected npm probe: {request.full_url}")
                if request.has_header("Authorization"):
                    raise AssertionError(
                        "public npm integrity probe received authorization"
                    )
                return Response(self.payload)

        def execute(payload: dict) -> str:
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / "release-bins").mkdir()
                (root / "release-bins" / filename).write_bytes(tarball_bytes)
                previous = Path.cwd()
                output = io.StringIO()
                try:
                    os.chdir(root)
                    with (
                        mock.patch.dict(
                            os.environ,
                            {
                                "EXPECTED_NPM_TARBALL": filename,
                                "EXPECTED_NPM_VERSION": version,
                                "NODE_AUTH_TOKEN": "must-not-enter-public-probe",
                            },
                            clear=False,
                        ),
                        mock.patch.object(
                            urllib.request, "build_opener", return_value=Opener(payload)
                        ),
                        redirect_stdout(output),
                    ):
                        exec(compiled, {})
                finally:
                    os.chdir(previous)
                return output.getvalue().strip()

        exact = {
            "name": "@maxkle1nz/m1nd",
            "version": version,
            "dist": {"integrity": integrity},
        }
        self.assertEqual(execute(exact), "exact_existing")
        foreign = {
            "name": "@maxkle1nz/m1nd",
            "version": version,
            "dist": {"integrity": "sha512-Zm9yZWlnbg=="},
        }
        with self.assertRaisesRegex(SystemExit, "foreign immutable npm tarball"):
            execute(foreign)

    def test_cargo_packages_are_built_once_sealed_and_published_as_exact_bytes(self):
        workflow = self.text(".github/workflows/release.yml")
        candidate = self.text("scripts/m1nd10_release_candidate.py")
        publisher = self.text("scripts/m1nd10_crates_io_upload.py")
        manifest = self.text("m1nd-mcp/Cargo.toml")
        build_script = self.text("m1nd-mcp/build.rs")
        self.assertIn("crate-artifact:", workflow)
        self.assertEqual(len(re.findall(r"(?m)^\s*cargo package\b", workflow)), 1)
        for name in ("m1nd-core", "m1nd-control", "m1nd-ingest", "m1nd-mcp"):
            self.assertEqual(
                len(
                    re.findall(
                        rf"(?m)^\s*-p\s+{re.escape(name)}(?:\s*\\)?\s*$", workflow
                    )
                ),
                1,
                f"{name} must be selected exactly once for workspace packaging",
            )
        self.assertIn('elif path.name.endswith(".crate")', candidate)
        self.assertIn('"cargo_packages": cargo_packages', candidate)
        self.assertIn('"cargo_packages_per_crate": 1', candidate)
        self.assertIn("clean-tag-plus-candidate-sealed-mcp-ui", candidate)
        self.assertIn("release-bins/*.crate", workflow)
        verification = self.job(
            ".github/workflows/release.yml", "crate-publish-verification"
        )
        publish = self.job(".github/workflows/release.yml", "publish")
        self.assertNotRegex(publish, r"(?m)^\s*cargo package\b")
        self.assertNotRegex(publish, r"(?m)^\s*cargo publish\b")
        self.assertNotRegex(publish, r"(?m)^\s*cargo check\b")
        self.assertNotIn("actions/checkout", publish)
        self.assertNotIn("scripts/", publish)
        self.assertIn("m1nd10_release_candidate.py verify", verification)
        self.assertIn("m1nd10_crates_io_upload.py extract", verification)
        self.assertIn("publisher.build_upload_body", verification)
        self.assertIn("m1nd-crates-publish-ready-${{ github.sha }}", verification)
        self.assertIn("m1nd-crates-publish-ready-${{ github.sha }}", publish)
        body_index = verification.index(
            "Build sealed crates.io request bodies and reject foreign existing bytes"
        )
        upload_index = verification.index(
            "name: m1nd-crates-publish-ready-${{ github.sha }}"
        )
        compile_index = verification.index(
            "Compile every exact package only after publish-ready bytes are immutable"
        )
        self.assertLess(body_index, upload_index)
        self.assertLess(upload_index, compile_index)
        self.assertIn("foreign immutable bytes already exist", verification)
        self.assertIn(
            '"registry_state": "absent" if observed_checksum is None else "exact_existing"',
            verification,
        )
        self.assertIn('"observed_checksum": observed_checksum', verification)
        positions = [
            publish.index(f'"m1nd-{suffix}"')
            for suffix in ("core", "control", "ingest", "mcp")
        ]
        self.assertEqual(positions, sorted(positions))
        self.assertIn(
            'CRATES_IO_UPLOAD_URL = "https://crates.io/api/v1/crates/new"', publisher
        )
        self.assertIn('"Authorization": token', publisher)
        self.assertIn("+ crate_bytes", publisher)
        self.assertIn('"ui-dist/**"', manifest)
        self.assertIn('"ui-package.json"', manifest)
        self.assertIn("m1nd_packaged_ui", build_script)
        self.assertIn("M1ND_EXPECTED_UI_BUNDLE_SHA256", verification)
        self.assertIn('"Authorization": token', publish)
        self.assertIn("permissions: {}", publish)
        self.assertIn("m1nd-release-candidate-${{ github.sha }}", publish)
        self.assertIn("cosign verify-blob", publish)
        self.assertIn('candidate.get("cargo_packages")', publish)
        self.assertIn('row.get("crate_sha256") != signed_sha256', publish)
        self.assertIn("crate_bytes != signed_crate_bytes", publish)
        self.assertIn("already published exact signed candidate bytes", publish)
        self.assertIn("recovered exact signed candidate publication", publish)
        self.assertIn("foreign immutable bytes exist", publish)
        self.assertIn("visible_checksum == signed_sha256", publish)
        self.assertNotIn("contents: read", publish)

    def test_candidate_signing_is_split_from_repository_code(self):
        assembly = self.job(".github/workflows/release.yml", "candidate-assembly")
        signing = self.job(".github/workflows/release.yml", "candidate")
        self.assertIn("actions/checkout", assembly)
        self.assertIn("persist-credentials: false", assembly)
        self.assertNotIn("id-token: write", assembly)
        self.assertNotIn("attestations: write", assembly)
        self.assertIn("m1nd-release-candidate-unsigned-${{ github.sha }}", assembly)
        self.assertNotIn("actions/checkout", signing)
        self.assertNotIn("scripts/", signing)
        self.assertIn("id-token: write", signing)
        self.assertIn("attestations: write", signing)
        self.assertIn("m1nd-release-candidate-unsigned-${{ github.sha }}", signing)
        self.assertIn("m1nd-release-candidate-${{ github.sha }}", signing)

    def test_release_verification_is_read_only_and_mutation_has_no_checkout(self):
        verification = self.job(".github/workflows/release.yml", "release-verification")
        mutation = self.job(".github/workflows/release.yml", "release")
        self.assertIn("contents: read", verification)
        self.assertIn("persist-credentials: false", verification)
        self.assertIn("m1nd10_release_candidate.py verify", verification)
        self.assertIn("verify-update-receipts", verification)
        self.assertNotIn("environment: release", verification)
        self.assertIn("release-verification", mutation.splitlines()[1])
        self.assertIn("contents: write", mutation)
        self.assertNotIn("actions/checkout", mutation)
        self.assertNotIn("scripts/", mutation)
        self.assertNotRegex(mutation, r"(?m)^\s+run:")
        self.assertIn("softprops/action-gh-release@", mutation)

    def test_registry_credentials_are_final_step_scoped(self):
        workflow = self.text(".github/workflows/release.yml")
        cargo_verify = self.job(
            ".github/workflows/release.yml", "crate-publish-verification"
        )
        cargo_publish = self.job(".github/workflows/release.yml", "publish")
        npm_publish = self.job(".github/workflows/release.yml", "publish-npm")
        self.assertEqual(workflow.count("secrets.CARGO_REGISTRY_TOKEN"), 1)
        self.assertEqual(workflow.count("secrets.NPM_TOKEN"), 1)
        self.assertNotIn("CARGO_REGISTRY_TOKEN", cargo_verify)
        self.assertNotRegex(cargo_publish, r"(?m)^    env:")
        self.assertNotRegex(npm_publish, r"(?m)^    env:")
        self.assertIn(
            "Upload and observe the four exact crates.io bodies", cargo_publish
        )
        self.assertIn(
            "CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}", cargo_publish
        )
        self.assertIn(
            "Publish the exact candidate tarball once without lifecycle scripts",
            npm_publish,
        )
        self.assertIn("NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}", npm_publish)
        self.assertIn('NPM_CONFIG_IGNORE_SCRIPTS: "true"', npm_publish)
        self.assertNotIn("actions/checkout", cargo_publish)
        self.assertNotIn("actions/checkout", npm_publish)
        self.assertNotIn("scripts/", cargo_publish)
        self.assertNotIn("scripts/", npm_publish)

    def test_every_release_checkout_drops_persisted_credentials(self):
        workflow = self.text(".github/workflows/release.yml")
        checkout_count = workflow.count("uses: actions/checkout@")
        self.assertGreater(checkout_count, 0)
        self.assertEqual(
            workflow.count("persist-credentials: false"),
            checkout_count,
            "every checkout in the release graph must remove its credential before repo code runs",
        )

    def test_release_inline_python_is_syntactically_valid(self):
        lines = self.text(".github/workflows/release.yml").splitlines()
        compiled = 0
        index = 0
        while index < len(lines):
            if "python3 - <<'PY'" not in lines[index]:
                index += 1
                continue
            start = index + 2
            index += 1
            body = []
            while index < len(lines) and lines[index].strip() != "PY":
                body.append(lines[index])
                index += 1
            self.assertLess(
                index, len(lines), f"unterminated Python heredoc at line {start}"
            )
            margins = [len(line) - len(line.lstrip()) for line in body if line.strip()]
            margin = min(margins, default=0)
            source = "\n".join(line[margin:] for line in body) + "\n"
            compile(source, f"release.yml:{start}", "exec")
            compiled += 1
            index += 1
        self.assertGreaterEqual(compiled, 1)

    def test_pages_is_split_and_fails_closed_without_unverified_pins(self):
        workflow = self.text(".github/workflows/deploy-wiki.yml")
        self.assert_immutable_uses(".github/workflows/deploy-wiki.yml")
        self.assertIn("contents: read", workflow)
        self.assertIn("Pages supply chain NOT_PROVEN", workflow)
        deploy = workflow.split("\n  deploy:\n", 1)[1]
        self.assertNotIn("actions/checkout", deploy)
        self.assertNotIn("npm ", deploy)
        self.assertNotIn("cargo ", deploy)
        self.assertIn("pages: write", deploy)
        self.assertIn("id-token: write", deploy)

    def test_pathos_has_no_push_pat_or_mutable_action(self):
        workflow = self.text(".github/workflows/pathos-autorefresh.yml")
        self.assert_immutable_uses(".github/workflows/pathos-autorefresh.yml")
        self.assertNotIn("contents: write", workflow)
        self.assertNotIn("PATHOS_REFRESH_TOKEN", workflow)
        self.assertNotIn("git push", workflow)
        self.assertNotIn("git commit", workflow)
        self.assertIn("persist-credentials: false", workflow)
        self.assertIn("pathos-pr-prep", workflow)


if __name__ == "__main__":
    unittest.main()
