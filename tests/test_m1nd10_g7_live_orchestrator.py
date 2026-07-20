from __future__ import annotations

import importlib.util
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "m1nd10_g7_live_orchestrator.py"
SPEC = importlib.util.spec_from_file_location("m1nd10_g7_live_orchestrator", SCRIPT)
assert SPEC and SPEC.loader
g7 = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(g7)


class G7LiveOrchestratorTests(unittest.TestCase):
    def make_source(self, root: Path) -> tuple[Path, str]:
        source = root / "source"
        (source / "m1nd-mcp").mkdir(parents=True)
        (source / "m1nd-ui" / "e2e-live").mkdir(parents=True)
        (source / "m1nd-ui" / "dist").mkdir()
        (source / "m1nd-ui" / "node_modules").mkdir()
        (source / ".gitignore").write_text("m1nd-ui/node_modules/\n", encoding="utf-8")
        (source / "Cargo.toml").write_text("[workspace]\n", encoding="utf-8")
        (source / "m1nd-mcp" / "Cargo.toml").write_text(
            '[package]\nname = "m1nd-mcp"\nversion = "1.4.0"\n', encoding="utf-8"
        )
        (source / "m1nd-ui" / "package.json").write_text(
            '{"name":"m1nd-ui","version":"1.4.0"}\n', encoding="utf-8"
        )
        (source / "m1nd-ui" / "package-lock.json").write_text(
            json.dumps(
                {
                    "name": "m1nd-ui",
                    "version": "1.4.0",
                    "lockfileVersion": 3,
                    "packages": {
                        "": {"name": "m1nd-ui", "version": "1.4.0"},
                        "node_modules/playwright": {
                            "version": "1.61.1",
                            "resolved": "https://registry.npmjs.org/playwright/-/playwright-1.61.1.tgz",
                            "integrity": "sha512-AAAA",
                        },
                        "node_modules/playwright-core": {
                            "version": "1.61.1",
                            "resolved": "https://registry.npmjs.org/playwright-core/-/playwright-core-1.61.1.tgz",
                            "integrity": "sha512-BBBB",
                        },
                    },
                }
            )
            + "\n",
            encoding="utf-8",
        )
        (source / "m1nd-ui" / "playwright.live.config.ts").write_text(
            "export default {};\n", encoding="utf-8"
        )
        (source / "m1nd-ui" / "e2e-live" / "live-owner.spec.ts").write_text(
            "// live\n", encoding="utf-8"
        )
        (source / "m1nd-ui" / "e2e-live" / "live-config.ts").write_text(
            "// config\n", encoding="utf-8"
        )
        (source / "m1nd-ui" / "e2e-live" / "live-test.ts").write_text(
            "// fixture\n", encoding="utf-8"
        )
        (source / "m1nd-ui" / "dist" / "index.html").write_text(
            "<main>m1nd</main>\n", encoding="utf-8"
        )
        (source / "m1nd-ui" / "dist" / "index.js").write_text(
            "console.log('m1nd');\n", encoding="utf-8"
        )
        (source / "graph_snapshot.json").write_text(
            '{"version":3,"nodes":[],"edges":[]}\n', encoding="utf-8"
        )
        (source / "plasticity_state.json").write_text("[]\n", encoding="utf-8")
        subprocess.run(["git", "init", "-q", str(source)], check=True)
        subprocess.run(["git", "-C", str(source), "add", "."], check=True)
        subprocess.run(
            [
                "git",
                "-C",
                str(source),
                "-c",
                "user.name=G7 Test",
                "-c",
                "user.email=g7@example.invalid",
                "commit",
                "-qm",
                "fixture",
            ],
            check=True,
        )
        source = source.resolve(strict=True)
        ui_digest, _ = g7.ui_tree_identity(source / "m1nd-ui" / "dist")
        return source, ui_digest

    def test_digest_normalization_requires_exact_lower_hex(self) -> None:
        digest = "a" * 64
        self.assertEqual(g7.normalize_sha256(digest, "digest"), digest)
        self.assertEqual(g7.normalize_sha256(f"sha256:{digest}", "digest"), digest)
        for invalid in ("A" * 64, "a" * 63, "sha512:" + digest):
            with self.assertRaises(g7.G7OrchestratorError):
                g7.normalize_sha256(invalid, "digest")

    def test_source_identity_binds_clean_root_tree_ui_and_version(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source, ui_digest = self.make_source(Path(temporary))
            identity = g7.inspect_source(source, ui_digest)
            self.assertRegex(identity["commit"], r"^[0-9a-f]{40}$")
            self.assertRegex(identity["tree"], r"^[0-9a-f]{40}$")
            self.assertEqual(identity["version"], "1.4.0")
            self.assertEqual(identity["ui_dist_sha256"], ui_digest)
            (source / "m1nd-ui" / "dist" / "index.js").write_text("changed\n")
            with self.assertRaisesRegex(g7.G7OrchestratorError, "dirty"):
                g7.inspect_source(source, ui_digest)

    def test_ephemeral_harness_comes_from_git_not_ignored_node_modules(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, _ = self.make_source(root)
            poison = source / "m1nd-ui" / "node_modules" / "poison.js"
            poison.write_text("throw new Error('ambient');\n", encoding="utf-8")
            self.assertEqual(
                subprocess.check_output(
                    ["git", "-C", str(source), "status", "--porcelain"], text=True
                ),
                "",
            )
            commit = subprocess.check_output(
                ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
            ).strip()
            workspace = g7.stage_git_ui_tree(source, commit, root / "ephemeral")
            self.assertFalse((workspace / "node_modules").exists())
            locked = g7.validate_locked_ui_dependencies(workspace)
            # execute() passes a pre-created TemporaryDirectory as destination;
            # staging into an existing empty root must succeed (the emptiness
            # guard is on destination/ui-harness, not the destination itself).
            pre_created = root / "pre-created-temp"
            pre_created.mkdir()
            workspace_existing = g7.stage_git_ui_tree(source, commit, pre_created)
            self.assertTrue(workspace_existing.is_dir())
            self.assertFalse((workspace_existing / "node_modules").exists())
            self.assertEqual(locked["dependency_count"], 2)
            self.assertEqual(
                locked["package_lock_sha256"],
                g7.sha256_file(source / "m1nd-ui" / "package-lock.json"),
            )

    def test_lock_requires_registry_sha512_for_every_dependency(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, _ = self.make_source(root)
            commit = subprocess.check_output(
                ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
            ).strip()
            workspace = g7.stage_git_ui_tree(source, commit, root / "ephemeral")
            lock_path = workspace / "package-lock.json"
            lock = json.loads(lock_path.read_text(encoding="utf-8"))
            del lock["packages"]["node_modules/playwright"]["integrity"]
            lock_path.write_text(json.dumps(lock), encoding="utf-8")
            with self.assertRaisesRegex(g7.G7OrchestratorError, "SHA-512"):
                g7.validate_locked_ui_dependencies(workspace)

    def test_tree_identity_binds_in_tree_symlink_and_refuses_escape(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            tree = root / "tree"
            tree.mkdir()
            (tree / "payload").write_text("bound\n", encoding="utf-8")
            (tree / "alias").symlink_to("payload")
            first = g7.framed_tree_identity(tree, b"test-domain\0", label="test tree")
            self.assertEqual(first[1], 2)
            (tree / "alias").unlink()
            (tree / "alias").symlink_to(root / "outside")
            (root / "outside").write_text("escape\n", encoding="utf-8")
            with self.assertRaisesRegex(g7.G7OrchestratorError, "symlink"):
                g7.framed_tree_identity(tree, b"test-domain\0", label="test tree")

    def test_absolute_inputs_refuse_relative_and_final_symlink(self) -> None:
        with self.assertRaisesRegex(g7.G7OrchestratorError, "absolute"):
            g7.absolute_regular_file(Path("relative"), "binary")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "target"
            target.write_text("binary")
            link = root / "link"
            link.symlink_to(target)
            with self.assertRaisesRegex(g7.G7OrchestratorError, "non-symlink"):
                g7.absolute_regular_file(link, "binary")

    def test_port_1338_is_refused_without_scanning(self) -> None:
        with self.assertRaisesRegex(g7.G7OrchestratorError, "1338"):
            g7.reserve_loopback_port(1338)
        reservation, selected = g7.reserve_loopback_port(None)
        try:
            self.assertNotEqual(selected, 1338)
            self.assertGreater(selected, 0)
        finally:
            reservation.close()
        with self.assertRaises(g7.G7OrchestratorError):
            g7.reserve_loopback_port(0)

    def test_owned_endpoint_comes_only_from_ephemeral_registry_and_exact_pid(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            registry = Path(temporary) / "registry"
            instances = registry / "instances"
            instances.mkdir(parents=True)
            process = SimpleNamespace(
                pid=os.getpid(), poll=lambda: None, returncode=None
            )
            entry = instances / "owner.json"
            entry.write_text(
                json.dumps(
                    {
                        "pid": process.pid,
                        "mode": "read_only",
                        "bind": "127.0.0.1",
                        "port": 18444,
                    }
                )
            )
            self.assertEqual(
                g7.wait_for_owned_endpoint(
                    process=process,
                    registry=registry,
                    requested_port=None,
                    timeout=0.2,
                ),
                18444,
            )
            entry.write_text(
                json.dumps(
                    {
                        "pid": process.pid,
                        "mode": "read_only",
                        "bind": "127.0.0.1",
                        "port": 1338,
                    }
                )
            )
            with self.assertRaisesRegex(g7.G7OrchestratorError, "1338"):
                g7.wait_for_owned_endpoint(
                    process=process,
                    registry=registry,
                    requested_port=None,
                    timeout=0.2,
                )

    def test_owned_endpoint_refuses_symlinked_instances_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            registry = root / "registry"
            outside = root / "outside"
            registry.mkdir()
            outside.mkdir()
            (registry / "instances").symlink_to(outside, target_is_directory=True)
            process = SimpleNamespace(
                pid=os.getpid(), poll=lambda: None, returncode=None
            )
            with self.assertRaisesRegex(g7.G7OrchestratorError, "symlink"):
                g7.wait_for_owned_endpoint(
                    process=process,
                    registry=registry,
                    requested_port=None,
                    timeout=0.2,
                )

    def test_owner_command_and_environment_are_explicit_isolated_read_only(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            prepared = {
                "root": root,
                "runtime": root / "runtime",
                "registry": root / "registry",
                "graph": root / "runtime" / "graph_snapshot.json",
                "plasticity": root / "runtime" / "plasticity_state.json",
            }
            command = g7.owner_command(Path("/opt/candidate/m1nd-mcp"), prepared, 18444)
            self.assertIn("--read-only", command)
            self.assertIn("--runtime-dir", command)
            self.assertIn(str(prepared["runtime"]), command)
            self.assertIn("--registry-dir", command)
            self.assertIn(str(prepared["registry"]), command)
            self.assertNotIn("--attach", command)
            self.assertNotIn("--ui-dir", command)
            self.assertNotIn("1338", command)

            old = os.environ.get("M1ND_ATTACH_URL")
            os.environ["M1ND_ATTACH_URL"] = "http://127.0.0.1:1338"
            try:
                environment = g7.owner_environment(
                    Path("/opt/source"),
                    prepared,
                    {"commit": "a" * 40, "version": "1.4.0"},
                )
            finally:
                if old is None:
                    os.environ.pop("M1ND_ATTACH_URL", None)
                else:
                    os.environ["M1ND_ATTACH_URL"] = old
            self.assertNotIn("M1ND_ATTACH_URL", environment)
            self.assertEqual(environment["M1ND_READ_ONLY"], "1")
            self.assertEqual(environment["M1ND_WORKSPACE_ROOT"], "/opt/source")
            self.assertEqual(environment["M1ND_RUNTIME_BASE"], str(root))

    def test_private_token_reader_refuses_permissions_and_symlink(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            token_path = root / "token"
            token_path.write_text("ab" * 32 + "\n", encoding="ascii")
            token_path.chmod(0o600)
            self.assertEqual(g7.read_private_token(token_path), "ab" * 32)
            token_path.chmod(0o644)
            with self.assertRaisesRegex(g7.G7OrchestratorError, "permissions"):
                g7.read_private_token(token_path)
            token_path.chmod(0o600)
            link = root / "link"
            link.symlink_to(token_path)
            with self.assertRaisesRegex(g7.G7OrchestratorError, "regular"):
                g7.read_private_token(link)

    def test_projection_binds_binary_source_bundle_manifest_and_served_root(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary).resolve()
            source_identity = {
                "commit": "a" * 40,
                "tree": "b" * 40,
                "version": "1.4.0",
            }
            binary_digest = "c" * 64
            ui_digest = "d" * 64
            sealed = "sha256:" + "e" * 64
            manifest = {
                "schema": g7.MANIFEST_SCHEMA,
                "repo_id": source.name,
                "project_root_fingerprint": g7.expected_project_fingerprint(source),
                "source": {
                    "commit": source_identity["commit"],
                    "dirty": False,
                    "version": "1.4.0",
                },
                "runtime": {
                    "binary_sha256": f"sha256:{binary_digest}",
                    "binary_version": "1.4.0",
                },
                "ui": {
                    "bundle_sha256": f"sha256:{ui_digest}",
                    "bundle_version": "1.4.0",
                    "mode": "embedded",
                },
                "authorities": {
                    "source": {
                        "digest": source_identity["commit"],
                        "revision": "1.4.0",
                        "status": "AVAILABLE",
                        "freshness": "FRESH",
                    },
                    "runtime_binary": {
                        "digest": f"sha256:{binary_digest}",
                        "revision": source_identity["commit"],
                        "status": "AVAILABLE",
                        "freshness": "FRESH",
                    },
                    "ui_bundle": {
                        "digest": f"sha256:{ui_digest}",
                        "revision": "1.4.0",
                        "status": "AVAILABLE",
                        "freshness": "FRESH",
                    },
                },
                "manifest_sha256": sealed,
            }
            response = {
                "schema": g7.OWNER_RESPONSE_SCHEMA,
                "manifest": manifest,
                "verification": {
                    "coherence": "COHERENT",
                    "computed_manifest_sha256": sealed,
                },
            }
            raw = json.dumps(response).encode()
            stats = {"served_brain": {"project_root": str(source)}}
            digests = g7.validate_owner_projection(
                response,
                raw,
                stats,
                source_root=source,
                source=source_identity,
                binary_sha256=binary_digest,
                expected_ui_sha256=ui_digest,
            )
            self.assertEqual(digests["bundle_sha256"], f"sha256:{ui_digest}")
            self.assertEqual(digests["manifest_sha256"], sealed)

            response["manifest"]["runtime"]["binary_sha256"] = "sha256:" + "f" * 64
            with self.assertRaisesRegex(g7.G7OrchestratorError, "binary digest"):
                g7.validate_owner_projection(
                    response,
                    raw,
                    stats,
                    source_root=source,
                    source=source_identity,
                    binary_sha256=binary_digest,
                    expected_ui_sha256=ui_digest,
                )

    def test_gate_environment_contains_token_path_but_no_bearer(self) -> None:
        prepared = {"runtime": Path("/private/tmp/g7/runtime")}
        environment = g7.gate_environment(
            Path("/opt/source"),
            prepared,
            18444,
            "a" * 64,
            Path("/opt/browser/chromium"),
        )
        self.assertEqual(environment["M1ND_LIVE_OWNER_URL"], "http://127.0.0.1:18444")
        self.assertEqual(
            environment["M1ND_LIVE_OWNER_TOKEN_FILE"],
            "/private/tmp/g7/runtime/http-auth-token-v1",
        )
        self.assertNotIn("M1ND_LIVE_OWNER_TOKEN", environment)
        self.assertEqual(environment["NPM_CONFIG_OFFLINE"], "true")
        self.assertEqual(
            environment["M1ND_LIVE_BROWSER_EXECUTABLE"],
            "/opt/browser/chromium",
        )

    def test_instance_self_proves_read_only_ephemeral_exact_source_binding(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source = root / "source"
            runtime = root / "runtime"
            registry = root / "registry"
            source.mkdir()
            runtime.mkdir()
            registry.mkdir()
            graph = runtime / "graph_snapshot.json"
            graph.write_text("{}\n")
            prepared = {
                "runtime": runtime,
                "registry": registry,
                "graph": graph,
            }
            response = {
                "instance": {
                    "workspace_root": str(source),
                    "runtime_root": str(runtime),
                    "graph_source": str(graph),
                    "mode": "read_only",
                    "port": 18444,
                },
                "graph_state": {
                    "runtime_root": str(runtime),
                    "graph_path": str(graph),
                    "workspace_root_source": "env:M1ND_WORKSPACE_ROOT",
                },
            }
            observed = g7.validate_owner_isolation(
                response,
                source_root=source,
                prepared=prepared,
                port=18444,
            )
            self.assertTrue(observed["read_only_observed"])
            response["instance"]["mode"] = "read_write"
            with self.assertRaisesRegex(g7.G7OrchestratorError, "read_only"):
                g7.validate_owner_isolation(
                    response,
                    source_root=source,
                    prepared=prepared,
                    port=18444,
                )

    def test_output_monitor_hashes_without_retaining_and_detects_secret(self) -> None:
        secret = b"a" * 64
        monitor = g7.OutputDigestMonitor(
            io.BytesIO(b"prefix" + secret + b"suffix"), secret
        )
        monitor.start()
        monitor.finish()
        receipt = monitor.receipt()
        self.assertTrue(receipt["token_detected"])
        self.assertNotIn(secret.decode(), json.dumps(receipt))

    @unittest.skipUnless(os.name == "posix", "POSIX process group contract")
    def test_created_process_group_is_fully_terminated(self) -> None:
        process = subprocess.Popen(
            [
                sys.executable,
                "-c",
                (
                    "import subprocess,sys,time; "
                    "child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)']); "
                    "print(child.pid,flush=True); time.sleep(60)"
                ),
            ],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
            text=True,
        )
        group = g7.assert_new_process_group(process, "test")
        assert process.stdout is not None
        child_pid = int(process.stdout.readline().strip())
        self.assertEqual(os.getpgid(child_pid), group)
        self.assertTrue(g7.process_group_exists(group))
        self.assertTrue(g7.terminate_process_group(process, group, timeout=2.0))
        process.stdout.close()
        self.assertFalse(g7.process_group_exists(group))

    def test_atomic_receipt_is_private_and_token_free(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "receipt.json"
            g7.atomic_receipt(path, {"schema": g7.SCHEMA, "status": "FAIL"})
            self.assertEqual(json.loads(path.read_text())["schema"], g7.SCHEMA)
            if os.name == "posix":
                self.assertEqual(path.stat().st_mode & 0o077, 0)

    def test_script_has_no_service_discovery_attach_or_network_install(self) -> None:
        source = SCRIPT.read_text(encoding="utf-8")
        self.assertNotIn("lsof", source)
        self.assertNotIn("pkill", source)
        self.assertNotIn("killall", source)
        self.assertNotIn('"--attach"', source)
        self.assertIn('"ci"', source)
        self.assertIn('"--offline"', source)
        self.assertIn('"--ignore-scripts"', source)
        self.assertNotRegex(source, r"npm\s+install|playwright\s+install")


if __name__ == "__main__":
    unittest.main()
