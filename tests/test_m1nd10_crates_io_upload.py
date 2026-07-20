import hashlib
import importlib.util
import io
import json
import struct
import tarfile
import tempfile
import unittest
import urllib.error
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "m1nd10_crates_io_upload", ROOT / "scripts" / "m1nd10_crates_io_upload.py"
)
assert SPEC and SPEC.loader
publisher = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publisher)


COMMIT = "a" * 40
VERSION = "1.4.0"
UI_FILES = {
    "assets/app.js": b"console.log('sealed m1nd');\n",
    "index.html": b"<!doctype html><main>sealed m1nd</main>\n",
}


def add_bytes(archive: tarfile.TarFile, name: str, payload: bytes, mode: int = 0o644) -> None:
    member = tarfile.TarInfo(name)
    member.mode = mode
    member.size = len(payload)
    archive.addfile(member, io.BytesIO(payload))


def write_crate(
    directory: Path,
    *,
    name: str = "m1nd-mcp",
    version: str = VERSION,
    dirty: bool = True,
    include_ui: bool = True,
    unsafe_member: str | None = None,
    link_member: bool = False,
) -> Path:
    path = directory / f"{name}-{version}.crate"
    root = f"{name}-{version}"
    manifest = f"""
[package]
name = {json.dumps(name)}
version = {json.dumps(version)}
edition = "2021"
authors = ["M1ND"]
description = "candidate sealed crate"
readme = "README.md"
keywords = ["agent"]
categories = ["development-tools"]
license = "MIT"
repository = "https://github.com/maxkle1nz/m1nd"
rust-version = "1.82"

[dependencies.m1nd-core]
version = "1.4.0"
default-features = false
features = ["embed"]

[features]
default = []
""".lstrip().encode()
    vcs = json.dumps({"git": {"sha1": COMMIT, "dirty": dirty}}).encode()
    lock = f"""
version = 4

[[package]]
name = {json.dumps(name)}
version = {json.dumps(version)}
dependencies = ["m1nd-core"]

[[package]]
name = "m1nd-core"
version = "1.4.0"
source = {json.dumps(publisher.CRATES_IO_LOCK_SOURCE)}
checksum = {json.dumps("b" * 64)}
""".lstrip().encode()
    with tarfile.open(path, "w:gz") as archive:
        add_bytes(archive, f"{root}/Cargo.toml", manifest)
        add_bytes(archive, f"{root}/Cargo.lock", lock)
        add_bytes(archive, f"{root}/.cargo_vcs_info.json", vcs)
        add_bytes(archive, f"{root}/README.md", b"sealed candidate\n")
        if include_ui:
            for relative, payload in UI_FILES.items():
                add_bytes(archive, f"{root}/ui-dist/{relative}", payload)
            add_bytes(
                archive,
                f"{root}/ui-package.json",
                json.dumps({"name": "m1nd-ui", "version": "0.1.0"}).encode(),
            )
        if unsafe_member is not None:
            add_bytes(archive, unsafe_member, b"escape")
        if link_member:
            member = tarfile.TarInfo(f"{root}/link")
            member.type = tarfile.SYMTYPE
            member.linkname = "Cargo.toml"
            archive.addfile(member)
    return path


class FakeResponse:
    def __init__(self, status: int, payload: bytes = b""):
        self.status = status
        self.payload = payload

    def __enter__(self):
        return self

    def __exit__(self, _type, _value, _traceback):
        return False

    def read(self, limit: int = -1) -> bytes:
        return self.payload if limit < 0 else self.payload[:limit]


class FakeOpener:
    def __init__(self, get_status: int = 404):
        self.get_status = get_status
        self.requests = []

    def open(self, request, timeout):
        self.requests.append((request, timeout))
        if request.get_method() == "GET":
            return FakeResponse(self.get_status)
        if request.get_method() == "PUT":
            return FakeResponse(200, b'{"warnings":{}}')
        raise AssertionError(f"unexpected method {request.get_method()}")


class ExactCratePublisherTests(unittest.TestCase):
    def test_inspect_binds_source_dependencies_and_packaged_ui(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = write_crate(Path(temporary))
            inspected = publisher.inspect_crate(path)
            self.assertEqual(inspected["name"], "m1nd-mcp")
            self.assertEqual(inspected["version"], VERSION)
            self.assertEqual(inspected["source_commit"], COMMIT)
            self.assertTrue(inspected["source_dirty"])
            self.assertEqual(inspected["ui_file_count"], len(UI_FILES))
            self.assertFalse(inspected["ui_placeholder"])
            self.assertEqual(inspected["ui_package_version"], "0.1.0")
            self.assertEqual(
                inspected["metadata"]["deps"][0],
                {
                    "default_features": False,
                    "features": ["embed"],
                    "kind": "normal",
                    "name": "m1nd-core",
                    "optional": False,
                    "target": None,
                    "version_req": "1.4.0",
                },
            )
            self.assertEqual(
                inspected["workspace_lock_dependencies"],
                [
                    {
                        "checksum": "b" * 64,
                        "name": "m1nd-core",
                        "source": publisher.CRATES_IO_LOCK_SOURCE,
                        "version": "1.4.0",
                    }
                ],
            )
            entry = publisher.candidate_artifact(path)
            self.assertEqual(entry["publish_order"], 4)
            self.assertEqual(entry["sha256"], hashlib.sha256(path.read_bytes()).hexdigest())
            self.assertEqual(entry["ui_bundle_sha256"], inspected["ui_bundle_sha256"])

    def test_upload_body_uses_documented_little_endian_framing_and_exact_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = write_crate(Path(temporary))
            body = publisher.build_upload_body(path)
            metadata_size = struct.unpack_from("<I", body, 0)[0]
            metadata_start = 4
            metadata_end = metadata_start + metadata_size
            metadata = json.loads(body[metadata_start:metadata_end])
            crate_size = struct.unpack_from("<I", body, metadata_end)[0]
            crate_start = metadata_end + 4
            self.assertEqual(metadata["name"], "m1nd-mcp")
            self.assertEqual(metadata["vers"], VERSION)
            self.assertEqual(crate_size, path.stat().st_size)
            self.assertEqual(body[crate_start:], path.read_bytes())

    def test_upload_probes_absence_then_puts_exact_candidate_body(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = write_crate(Path(temporary))
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            opener = FakeOpener()
            result = publisher.upload_exact_crate(
                path,
                expected_name="m1nd-mcp",
                expected_version=VERSION,
                expected_sha256=digest,
                token="secret-token",
                opener=opener,
            )
            self.assertEqual(result, {"warnings": {}})
            self.assertEqual([request.get_method() for request, _ in opener.requests], ["GET", "PUT"])
            upload = opener.requests[1][0]
            self.assertEqual(upload.full_url, publisher.CRATES_IO_UPLOAD_URL)
            self.assertEqual(upload.get_header("Authorization"), "secret-token")
            self.assertEqual(upload.data, publisher.build_upload_body(path))

    def test_existing_or_tampered_identity_refuses_before_upload(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = write_crate(Path(temporary))
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            existing = FakeOpener(get_status=200)
            with self.assertRaisesRegex(publisher.CratePackageError, "already exists"):
                publisher.upload_exact_crate(
                    path,
                    expected_name="m1nd-mcp",
                    expected_version=VERSION,
                    expected_sha256=digest,
                    token="secret-token",
                    opener=existing,
                )
            self.assertEqual(len(existing.requests), 1)

            untouched = FakeOpener()
            with self.assertRaisesRegex(publisher.CratePackageError, "differs from candidate"):
                publisher.upload_exact_crate(
                    path,
                    expected_name="m1nd-mcp",
                    expected_version=VERSION,
                    expected_sha256="b" * 64,
                    token="secret-token",
                    opener=untouched,
                )
            self.assertEqual(untouched.requests, [])

    def test_archive_paths_links_and_redirects_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            unsafe = write_crate(directory, unsafe_member="../escape")
            with self.assertRaisesRegex(publisher.CratePackageError, "unsafe path"):
                publisher.inspect_crate(unsafe)

        with tempfile.TemporaryDirectory() as temporary:
            linked = write_crate(Path(temporary), link_member=True)
            with self.assertRaisesRegex(publisher.CratePackageError, "links/special"):
                publisher.inspect_crate(linked)

        handler = publisher.RefuseRedirects()
        with self.assertRaises(urllib.error.HTTPError) as captured:
            handler.redirect_request(
                type("Request", (), {"full_url": publisher.CRATES_IO_UPLOAD_URL})(),
                None,
                302,
                "Found",
                {},
                "https://attacker.invalid/steal",
            )
        captured.exception.close()

    def test_safe_extract_preserves_only_valid_members(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            path = write_crate(directory)
            root = publisher.extract_exact_crate(path, directory / "extract")
            self.assertEqual(root.name, f"m1nd-mcp-{VERSION}")
            self.assertEqual((root / "ui-dist" / "index.html").read_bytes(), UI_FILES["index.html"])
            with self.assertRaisesRegex(publisher.CratePackageError, "already exists"):
                publisher.extract_exact_crate(path, directory / "extract")


if __name__ == "__main__":
    unittest.main()
