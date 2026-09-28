#!/usr/bin/env python3
"""Focused portable-devkit behavior checks using only neutral temporary fixtures."""

from __future__ import annotations

import os
import shutil
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEV = ROOT / "scripts" / "m1nd-dev"
ACTIVATE = ROOT / "scripts" / "devkit" / "activate.sh"
TARGET_SCRIPT = ROOT / "scripts" / "cargo_target_dir.sh"


class DevkitTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.temp = Path(self.temporary.name)
        self.checkout = self.temp / "repo-alpha"
        self.checkout.mkdir()
        subprocess.run(["git", "init", "-q"], cwd=self.checkout, check=True)
        subprocess.run(["git", "config", "user.email", "devkit@example.test"], cwd=self.checkout, check=True)
        subprocess.run(["git", "config", "user.name", "Devkit Test"], cwd=self.checkout, check=True)
        (self.checkout / "scripts").mkdir()
        shutil.copy2(TARGET_SCRIPT, self.checkout / "scripts" / "cargo_target_dir.sh")
        (self.checkout / "npm" / "bin").mkdir(parents=True)
        (self.checkout / "npm" / "bin" / "m1nd.js").write_text("// fixture entrypoint\n", encoding="utf-8")
        (self.checkout / "scripts" / "cargo").write_text(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> \"$FAKE_CARGO_LOG\"\n"
            "[[ -n \"${FAKE_CARGO_STAMP_LOG:-}\" ]] && printf '%s %s\\n' \"${M1ND_DEVKIT_BUILD_STAMP_REVISION:-}\" \"${M1ND_DEVKIT_BUILD_STAMP_DIRTY:-}\" >> \"$FAKE_CARGO_STAMP_LOG\"\n"
            "[[ \"${FAKE_MUTATE_DURING_BUILD:-}\" == 1 && \"${1:-}\" == build ]] && printf 'mid-build mutation\\n' >> \"$M1ND_CHECKOUT/tracked.txt\"\n"
            "[[ \"${1:-}\" == clean ]] && : > \"$FAKE_CARGO_CLEAN_MARKER\"\ntrue\n",
            encoding="utf-8",
        )
        (self.checkout / "scripts" / "cargo").chmod(0o755)
        (self.checkout / "tracked.txt").write_text("base\n", encoding="utf-8")
        subprocess.run(["git", "add", "."], cwd=self.checkout, check=True)
        subprocess.run(["git", "commit", "-qm", "fixture"], cwd=self.checkout, check=True)
        self.home = self.temp / "home"
        self.home.mkdir()
        self.state_base = self.temp / "state"
        self.events = self.temp / "events.jsonl"
        self.cargo_events = self.temp / "cargo-events.txt"
        self.stamp_events = self.temp / "cargo-stamp-events.txt"
        self.clean_marker = self.temp / "cargo-clean-marker"
        self.python = self.make_python_wrapper()
        self.node = self.make_node_wrapper()
        self.rustc = self.make_rustc_wrapper()
        self.prepare_fake_binary("release")
        self.prepare_fake_binary("dev-fast")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def make_python_wrapper(self) -> Path:
        tools = self.temp / "tool-bin"
        tools.mkdir(exist_ok=True)
        path = tools / "python3"
        path.write_text(
            f"#!{os.sys.executable}\n" + """
import os, pathlib, sys
model = os.environ.get("M1ND_DEV_MODEL_DIR", "")
if os.environ.get("FAKE_TREE_FINGERPRINT_FAILURE") == "1" and len(sys.argv) == 3 and sys.argv[1] == "-" and sys.argv[2] == os.environ["M1ND_CHECKOUT"]:
    raise SystemExit("fixture tree fingerprint failure")
if len(sys.argv) == 3 and sys.argv[1] == "-" and sys.argv[2].startswith(model):
    sys.stdin.read()
    print({
        "config.json": "2a6ac0e9aaa356a68a5688070db78fc3a464fefe85d2f06a1905ce3718687553",
        "model.safetensors": "f65d0f325faadc1e121c319e2faa41170d3fa07d8c89abd48ca5358d9a223de2",
        "tokenizer.json": "e67e803f624fb4d67dea1c730d06e1067e1b14d830e2c2202569e3ef0f70bb50",  # gitleaks:allow -- public CI-pinned tokenizer SHA-256, not a credential
    }[pathlib.Path(sys.argv[2]).name])
    raise SystemExit(0)
real = os.environ["DEVKIT_REAL_PYTHON"]
os.execv(real, [real, *sys.argv[1:]])
""",
            encoding="utf-8",
        )
        path.chmod(0o755)
        return path

    def make_node_wrapper(self) -> Path:
        path = self.temp / "tool-bin" / "node"
        path.write_text(
            f"#!{os.sys.executable}\n" + """
import json, os, pathlib, sys
if sys.argv[1:2] == ["-p"]:
    print("0")
    raise SystemExit(0)
pathlib.Path(os.environ["DEVKIT_EVENT_LOG"]).open("a").write(json.dumps({
  "argv": sys.argv[1:], "cwd": os.getcwd(), "home": os.environ.get("HOME"),
  "binary": os.environ.get("M1ND_MCP_BINARY"), "attach": os.environ.get("M1ND_ATTACH_URL"),
  "runtime": os.environ.get("M1ND_RUNTIME_DIR"), "registry": os.environ.get("M1ND_REGISTRY_DIR"),
  "agent_cache": os.environ.get("M1ND_AGENT_CACHE_DIR")
}) + "\\n")
""",
            encoding="utf-8",
        )
        path.chmod(0o755)
        return path

    def make_rustc_wrapper(self) -> Path:
        path = self.temp / "tool-bin" / "rustc"
        path.write_text("#!/usr/bin/env bash\nprintf '%s\n' 'rustc 1.98.1 (fixture)'\n", encoding="utf-8")
        path.chmod(0o755)
        return path

    def target(self) -> Path:
        output = subprocess.check_output(
            ["bash", "scripts/cargo_target_dir.sh"], cwd=self.checkout, env={**os.environ, "HOME": str(self.home)}, text=True
        ).strip()
        return Path(output)

    def prepare_fake_binary(self, target_profile: str) -> None:
        binary = self.target() / target_profile / "m1nd-mcp"
        binary.parent.mkdir(parents=True)
        binary.write_text(
            f"#!{os.sys.executable}\n" + """
import json, os, pathlib, subprocess, sys
head = subprocess.check_output(["git", "-C", os.environ["M1ND_CHECKOUT"], "rev-parse", "HEAD"], text=True).strip()
if subprocess.check_output(["git", "-C", os.environ["M1ND_CHECKOUT"], "status", "--porcelain"], text=True):
    head += "-dirty"
if sys.argv[1:] == ["--version"]:
    marker = os.environ.get("FAKE_CARGO_CLEAN_MARKER")
    if os.environ.get("FAKE_STALE_UNTIL_CLEAN") == "1" and marker and not os.path.exists(marker):
        print("m1nd-mcp fixture (stale)")
        raise SystemExit(0)
    print("m1nd-mcp fixture (" + head + ")")
    raise SystemExit(0)
if "--discover-owner" in sys.argv:
    pathlib.Path(os.environ["DEVKIT_EVENT_LOG"]).open("a").write(json.dumps({
      "kind": "discover", "argv": sys.argv[1:], "cwd": os.getcwd(), "home": os.environ.get("HOME"),
      "attach": os.environ.get("M1ND_ATTACH_URL"), "runtime": os.environ.get("M1ND_RUNTIME_DIR"),
      "registry": os.environ.get("M1ND_REGISTRY_DIR"), "bearer": os.environ.get("M1ND_HTTP_BEARER_TOKEN")
    }) + "\\n")
    mode = os.environ.get("FAKE_DISCOVERY", "direct")
    if mode == "direct":
        print(json.dumps({"schema":"m1nd-owner-discovery-v0","found":False,"reason":"no live serve owner for this client, on either discovery question: fixture"}))
        raise SystemExit(1)
    if mode == "attach":
        print(json.dumps({"schema":"m1nd-owner-discovery-v0","found":True,"discovery":"runtime_root","base_url":"http://127.0.0.1:14438","owner_runtime_root":"/fixture/owner"}))
        raise SystemExit(0)
    print("{}")
    raise SystemExit(1)
pathlib.Path(os.environ["DEVKIT_EVENT_LOG"]).open("a").write(json.dumps({
  "argv": sys.argv[1:], "cwd": os.getcwd(), "home": os.environ.get("HOME"),
  "attach": os.environ.get("M1ND_ATTACH_URL"), "runtime": os.environ.get("M1ND_RUNTIME_DIR"),
  "registry": os.environ.get("M1ND_REGISTRY_DIR"), "bearer": os.environ.get("M1ND_HTTP_BEARER_TOKEN")
}) + "\\n")
""",
            encoding="utf-8",
        )
        binary.chmod(0o755)

    def model_dir(self) -> Path:
        directory = self.temp / "model"
        directory.mkdir(exist_ok=True)
        for name in ("config.json", "model.safetensors", "tokenizer.json"):
            (directory / name).write_text("fixture", encoding="utf-8")
        return directory

    def environment(self, **more: str) -> dict[str, str]:
        env = {
            **os.environ,
            "HOME": str(self.home),
            "M1ND_CHECKOUT": str(self.checkout),
            "M1ND_DEV_STATE_BASE": str(self.state_base),
            "M1ND_PYTHON": str(self.python),
            "M1ND_NODE": str(self.node),
            "M1ND_NODE_MAJOR": "0",
            "M1ND_PYTHON_MINOR": f"{os.sys.version_info.major}.{os.sys.version_info.minor}",
            "M1ND_RUST_TOOLCHAIN": "1.98.1",
            "M1ND_DEV_MODEL_DIR": str(self.model_dir()),
            "DEVKIT_EVENT_LOG": str(self.events),
            "FAKE_CARGO_LOG": str(self.cargo_events),
            "FAKE_CARGO_STAMP_LOG": str(self.stamp_events),
            "FAKE_CARGO_CLEAN_MARKER": str(self.clean_marker),
            "DEVKIT_REAL_PYTHON": os.sys.executable,
            "RUSTUP_HOME": os.environ.get("RUSTUP_HOME", str(Path.home() / ".rustup")),
            "CARGO_HOME": os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")),
            **more,
        }
        return env

    def command(self, *args: str, **more: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run([str(DEV), *args], cwd=self.temp, env=self.environment(**more), text=True, capture_output=True, timeout=10)

    def events_read(self) -> list[dict]:
        return [__import__("json").loads(line) for line in self.events.read_text().splitlines()] if self.events.exists() else []

    def test_state_is_private_checkout_scoped_and_target_is_canonical(self) -> None:
        result = self.command("state")
        self.assertEqual(result.returncode, 0, result.stderr)
        state = Path(result.stdout.strip().split(": ", 1)[1])
        self.assertTrue(state.is_dir())
        self.assertEqual(stat.S_IMODE(state.stat().st_mode), 0o700)
        self.assertNotEqual(state, self.home / ".m1nd")
        expected = self.target()
        sourced = subprocess.check_output(
            ["bash", "-lc", f'source "{ACTIVATE}"; printf "%s" "$CARGO_TARGET_DIR"'],
            env=self.environment(), text=True,
        )
        self.assertEqual(sourced, str(expected))

    def test_activation_uses_the_override_toplevel_from_an_unrelated_cwd(self) -> None:
        outside = self.temp / "outside"
        outside.mkdir()
        sourced = subprocess.check_output(
            ["bash", "-lc", f'cd "{outside}"; source "{ACTIVATE}"; printf "%s" "$CARGO_TARGET_DIR"'],
            env=self.environment(), text=True,
        )
        self.assertEqual(sourced, str(self.target()))
        selected = subprocess.check_output(
            ["bash", "-lc", f'source "{ACTIVATE}"; command -v node; command -v python3; printf "%s\n" "$M1ND_DEV_PROFILE"'],
            env=self.environment(), text=True,
        ).splitlines()
        self.assertEqual(selected, [str(self.node), str(self.python), "fast"])
        nested = self.checkout / "nested"
        nested.mkdir()
        refused = subprocess.run(
            ["bash", "-lc", f'source "{ACTIVATE}"'],
            env=self.environment(M1ND_CHECKOUT=str(nested)), text=True, capture_output=True,
        )
        self.assertNotEqual(refused.returncode, 0)
        self.assertIn("toplevel", refused.stderr)

    def test_two_worktrees_receive_distinct_canonical_targets(self) -> None:
        sibling = self.temp / "repo-beta"
        subprocess.run(["git", "worktree", "add", "-q", "-b", "fixture-beta", str(sibling)], cwd=self.checkout, check=True)
        first = self.target().resolve()
        second = Path(subprocess.check_output(
            ["bash", "-lc", f'source "{ACTIVATE}"; printf "%s" "$CARGO_TARGET_DIR"'],
            env=self.environment(M1ND_CHECKOUT=str(sibling)), text=True,
        )).resolve()
        self.assertNotEqual(first, second)

    def test_private_state_symlink_is_refused_without_touching_its_target(self) -> None:
        target = self.temp / "untouched"
        target.write_text("sentinel", encoding="utf-8")
        target.chmod(0o644)
        linked = self.temp / "linked-state"
        linked.symlink_to(target)
        result = self.command("state", M1ND_DEV_STATE=str(linked))
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(target.read_text(encoding="utf-8"), "sentinel")
        self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o644)

    def test_state_override_under_checkout_or_live_runtime_is_refused_before_mutation(self) -> None:
        unsafe = self.checkout / "m1nd-ui" / "public" / "runtime"
        result = self.command("state", M1ND_DEV_STATE=str(unsafe))
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(unsafe.exists())
        live = self.home / ".m1nd" / "runtime"
        result = self.command("state", M1ND_DEV_STATE=str(live))
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(live.exists())

    def test_dirty_binary_requires_private_tree_fingerprint(self) -> None:
        (self.checkout / "tracked.txt").write_text("edited\n", encoding="utf-8")
        stale = self.command("version", "release")
        self.assertNotEqual(stale.returncode, 0)
        self.assertIn("does not match", stale.stderr)
        # A -dirty version string alone cannot certify any dirty edit; the
        # private fingerprint is written only after a successful one-pass build.

    def test_dirty_build_records_current_fingerprint_without_an_unneeded_clean(self) -> None:
        tracked = self.checkout / "tracked.txt"
        tracked.write_text("first dirty edit\n", encoding="utf-8")
        fingerprint_failed = self.command("build", FAKE_TREE_FINGERPRINT_FAILURE="1")
        self.assertNotEqual(fingerprint_failed.returncode, 0)
        self.assertFalse(list(self.state_base.rglob("fast.identity")))
        first = self.command("build")
        self.assertEqual(first.returncode, 0, first.stderr)
        tracked.write_text("second dirty edit\n", encoding="utf-8")
        second = self.command("build")
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(self.cargo_events.read_text(encoding="utf-8").splitlines(), [
            "build --locked --profile dev-fast -p m1nd-mcp",
            "build --locked --profile dev-fast -p m1nd-mcp",
        ])
        revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=self.checkout, text=True).strip()
        self.assertEqual(self.stamp_events.read_text(encoding="utf-8").splitlines(), [
            f"{revision} 1",
            f"{revision} 1",
        ])
        manifest = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
        self.assertIn("[profile.dev-fast]", manifest)
        self.assertIn("codegen-units = 256", manifest)
        build_script = (ROOT / "m1nd-mcp" / "build.rs").read_text(encoding="utf-8")
        self.assertIn('args(["rev-parse", "--git-path", pathspec])', build_script)
        self.assertIn('["HEAD", "index", "packed-refs"]', build_script)
        self.assertIn('args(["symbolic-ref", "-q", "HEAD"])', build_script)
        self.assertIn("watch_git_path(&manifest_dir, &current_ref)", build_script)
        self.assertIn("path.is_file()", build_script)
        self.assertNotIn('rerun-if-changed=../.git/HEAD', build_script)
        fingerprint_version_failed = self.command("version", FAKE_TREE_FINGERPRINT_FAILURE="1")
        self.assertNotEqual(fingerprint_version_failed.returncode, 0)
        self.assertIn("does not match", fingerprint_version_failed.stderr)

        identity = next(self.state_base.rglob("fast.identity"))
        receipt_before = identity.read_text(encoding="utf-8")
        changed = self.command("build", FAKE_MUTATE_DURING_BUILD="1")
        self.assertNotEqual(changed.returncode, 0)
        self.assertIn("changed during build", changed.stderr)
        self.assertEqual(identity.read_text(encoding="utf-8"), receipt_before)
        self.assertEqual(self.cargo_events.read_text(encoding="utf-8").splitlines()[-1], "build --locked --profile dev-fast -p m1nd-mcp")

    def test_build_performs_one_package_clean_rebuild_for_stale_build_identity(self) -> None:
        result = self.command("build", "release", FAKE_STALE_UNTIL_CLEAN="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.cargo_events.read_text(encoding="utf-8").splitlines(), [
            "build --locked --release -p m1nd-mcp",
            "clean -p m1nd-mcp",
            "build --locked --release -p m1nd-mcp",
        ])

    def test_stdio_uses_the_same_private_registry_as_serve_discovery_attach_and_cli(self) -> None:
        served = self.command("serve", "release")
        self.assertEqual(served.returncode, 0, served.stderr)
        serve_event = self.events_read()[-1]
        self.assertIn("--serve", serve_event["argv"])
        self.assertEqual(serve_event["registry"], str(Path(serve_event["runtime"]) / "registry"))
        result = self.command(
            "stdio", "release",
            M1ND_ATTACH_URL="http://unexpected.test", M1ND_HTTP_BEARER_TOKEN="not-forwarded",
            M1ND_RUNTIME_DIR="not-forwarded", FAKE_DISCOVERY="direct",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        discovery, event = self.events_read()[-2:]
        self.assertEqual(discovery["kind"], "discover")
        self.assertEqual(discovery["runtime"], serve_event["runtime"])
        self.assertEqual(discovery["registry"], serve_event["registry"])
        self.assertEqual(discovery["argv"][-2:], ["--registry-dir", serve_event["registry"]])
        self.assertIn("--stdio", event["argv"])
        self.assertNotIn("--attach", event["argv"])
        self.assertIsNone(event["attach"])
        self.assertNotEqual(event["runtime"], "not-forwarded")
        self.assertEqual(event["runtime"], serve_event["runtime"])
        self.assertEqual(event["registry"], serve_event["registry"])
        attached = self.command("stdio", "release", FAKE_DISCOVERY="attach")
        self.assertEqual(attached.returncode, 0, attached.stderr)
        discovery, attached_event = self.events_read()[-2:]
        self.assertEqual(discovery["kind"], "discover")
        self.assertEqual(discovery["registry"], serve_event["registry"])
        self.assertIn("--attach", attached_event["argv"])
        self.assertEqual(attached_event["registry"], serve_event["registry"])
        cli = self.command("cli", "doctor")
        self.assertEqual(cli.returncode, 0, cli.stderr)
        cli_event = self.events_read()[-1]
        self.assertEqual(cli_event["runtime"], serve_event["runtime"])
        self.assertEqual(cli_event["registry"], serve_event["registry"])
        malformed = self.command("stdio", "release", FAKE_DISCOVERY="malformed")
        self.assertNotEqual(malformed.returncode, 0)
        self.assertIn("refusing stdio", malformed.stderr)

    def test_cli_preserves_caller_cwd_and_home_while_using_private_binary(self) -> None:
        caller = self.temp / "caller"
        caller.mkdir()
        result = subprocess.run(
            [str(DEV), "cli", "doctor"], cwd=caller, env=self.environment(),
            text=True, capture_output=True, timeout=10,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        event = self.events_read()[-1]
        self.assertEqual(event["cwd"], str(caller.resolve()))
        self.assertEqual(event["home"], str(self.home))
        self.assertTrue(event["binary"].endswith("/dev-fast/m1nd-mcp"))
        self.assertEqual(event["argv"][-2:], ["--binary", event["binary"]])
        self.assertEqual(event["agent_cache"], str((self.state_base / next(self.state_base.iterdir()).name / "agent-cache").resolve()))
        self.assertEqual(event["runtime"], str(Path(event["agent_cache"]).parent / "runtime"))
        self.assertEqual(event["registry"], str(Path(event["runtime"]) / "registry"))
        event_count = len(self.events_read())
        rejected = subprocess.run(
            [str(DEV), "cli", "doctor", "--binary", str(self.temp / "foreign-m1nd-mcp")], cwd=caller,
            env=self.environment(), text=True, capture_output=True, timeout=10,
        )
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("binds --binary", rejected.stderr)
        self.assertEqual(len(self.events_read()), event_count)
        for blocked_args, message in [
            (["restart"], "refuses restart"),
            (["update", "apply", "--yes"], "refuses restart"),
            (["/restart", "--yes"], "refuses restart"),
            (["--json", "restart", "--yes"], "supported command as its first"),
        ]:
            blocked = subprocess.run(
                [str(DEV), "cli", *blocked_args], cwd=caller, env=self.environment(),
                text=True, capture_output=True, timeout=10,
            )
            self.assertNotEqual(blocked.returncode, 0)
            self.assertIn(message, blocked.stderr)
            self.assertEqual(len(self.events_read()), event_count)

    def test_invalid_port_fails_before_starting_native_process(self) -> None:
        result = self.command("serve", "release", M1ND_DEV_HTTP_PORT="65536")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("integer", result.stderr)
        self.assertEqual(self.events_read(), [])


if __name__ == "__main__":
    unittest.main()
