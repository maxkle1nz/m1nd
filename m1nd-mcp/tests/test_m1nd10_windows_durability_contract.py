from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
MCP = ROOT / "m1nd-mcp"


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


class WindowsDurabilityContract(unittest.TestCase):
    def test_windows_native_primitives_are_declared_and_centralized(self) -> None:
        manifest = read("m1nd-mcp/Cargo.toml")
        helper = read("m1nd-mcp/src/windows_durable_fs.rs")

        self.assertIn("[target.'cfg(windows)'.dependencies]", manifest)
        self.assertIn('windows-sys = { version = "0.61.2"', manifest)
        for feature in (
            "Wdk_Foundation",
            "Wdk_Storage_FileSystem",
            "Win32_Foundation",
            "Win32_Security",
            "Win32_Storage_FileSystem",
            "Win32_System_IO",
        ):
            self.assertIn(feature, manifest)

        for primitive in (
            "GetFileInformationByHandle",
            "FILE_ATTRIBUTE_REPARSE_POINT",
            "FILE_FLAG_OPEN_REPARSE_POINT",
            "LockFileEx",
            "UnlockFileEx",
            "MoveFileExW",
            "MOVEFILE_WRITE_THROUGH",
            "MOVEFILE_REPLACE_EXISTING",
            "NtCreateFile",
            "OBJECT_ATTRIBUTES",
            "FILE_OPEN_REPARSE_POINT as NT_FILE_OPEN_REPARSE_POINT",
        ):
            self.assertIn(primitive, helper)
        self.assertNotIn("ReplaceFileW", helper)
        self.assertNotIn("FILE_SHARE_DELETE", helper)


    def test_checkpoint_windows_path_is_fail_closed_and_write_through(self) -> None:
        checkpoint = read("m1nd-mcp/src/checkpoint_store.rs")

        self.assertIn("crate::windows_durable_fs::directory_identity(file)", checkpoint)
        self.assertIn("crate::windows_durable_fs::open_directory_no_follow(path)", checkpoint)
        self.assertIn("crate::windows_durable_fs::lock_file_exclusive(file, true)", checkpoint)
        self.assertIn(
            "crate::windows_durable_fs::move_new_write_through(source, destination)",
            checkpoint,
        )
        self.assertIn(
            "crate::windows_durable_fs::replace_write_through(source, destination)",
            checkpoint,
        )
        self.assertIn('".gc-{checkpoint_id}-{}-{nonce}"', checkpoint)
        self.assertIn("let _ = fs::remove_dir_all(path);", checkpoint)
        self.assertNotIn("fn sync_directory(_path: &Path)", checkpoint)
        self.assertNotIn("\n    root: PathBuf,", checkpoint)
        self.assertIn("&self.inner.namespace_root", checkpoint)
        self.assertIn("self.inner.checkpoints.join(checkpoint_id)", checkpoint)

    def test_graph_ingest_windows_candidate_is_handle_anchored_and_durable(self) -> None:
        graph_ingest = read(
            "m1nd-mcp/src/external_mutation_service/graph_ingest_a2.rs"
        )
        helper = read("m1nd-mcp/src/windows_durable_fs.rs")

        for contract in (
            "WindowsCandidateArtifactAnchor",
            "open_relative_directory_no_follow",
            "create_relative_new_no_follow",
            "open_relative_read_no_follow",
            "handle_identity(&file)",
            "move_new_write_through(&staged_path, &final_path)",
            "file.write_all(&bytes)",
            "file.sync_all()",
            "#[cfg(not(any(unix, windows)))]",
        ):
            self.assertIn(contract, graph_ingest)
        self.assertIn("RootDirectory: parent.as_raw_handle() as HANDLE", helper)
        self.assertIn("NT_FILE_OPEN_REPARSE_POINT", helper)
        self.assertIn("FILE_NON_DIRECTORY_FILE", helper)
        self.assertIn("FILE_ATTRIBUTE_REPARSE_POINT", helper)
        self.assertIn("GRAPH_INGEST_A2_MAX_CANDIDATE_BYTES", graph_ingest)
        self.assertNotIn("#[cfg(not(unix))]", graph_ingest)
        self.assertLess(
            graph_ingest.index("staged.load_durable_candidate(false)?"),
            graph_ingest.index("pub(super) fn request_matches_entry"),
        )


    def test_durable_journals_do_not_claim_windows_directory_fsync(self) -> None:
        authority = read("m1nd-mcp/src/authority_wal.rs")
        external = read("m1nd-mcp/src/external_mutation_journal.rs")
        evidence = read("m1nd-mcp/src/evidence_spine.rs")
        lock_guard = read("m1nd-mcp/src/light_author_handlers.rs")

        self.assertIn("open_read_append_create_no_follow(path)", authority)
        self.assertIn("open_read_append_create_no_follow(path)", external)
        self.assertIn("replace_write_through(source, destination)", evidence)
        self.assertIn("open_create_new_no_follow(path)", evidence)
        self.assertLess(evidence.index("drop(file);"), evidence.index("replace_atomic_write_through"))
        self.assertIn("lock_file_exclusive(&file, false)", lock_guard)
        self.assertIn("libc::O_NOFOLLOW | libc::O_CLOEXEC", lock_guard)

        for source in (authority, external, evidence):
            self.assertNotIn("reviewed directory fsync primitive on Windows", source)
            self.assertNotIn("fn sync_parent_directory(_parent: &Path)", source)


if __name__ == "__main__":
    unittest.main()
