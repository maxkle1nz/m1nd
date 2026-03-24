// === Ultra Edit Phase 1: edit_preview + edit_commit tests ===
//
// 7 mandatory tests covering the two-phase preview/commit flow:
//   1. preview_happy_path       — preview returns handle, diff, snapshot
//   2. preview_nonexistent_file — preview on missing file, file_exists=false
//   3. commit_happy_path        — preview + commit(confirm=true), file written
//   4. commit_handle_expired    — preview with old timestamp, TTL rejects
//   5. commit_source_modified   — file changed after preview, hash mismatch
//   6. commit_confirm_false     — confirm=false rejected
//   7. commit_handle_not_found  — bogus preview_id rejected

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::protocol::surgical::{EditCommitInput, EditPreviewInput};
use m1nd_mcp::server::McpConfig;
use m1nd_mcp::session::SessionState;
use m1nd_mcp::surgical_handlers::{handle_edit_commit, handle_edit_preview};
use std::path::{Path, PathBuf};

// ===========================================================================
// Test infrastructure
// ===========================================================================

fn make_test_state(root: &Path) -> SessionState {
    let mut config = McpConfig::default();
    config.graph_source = root.join("graph_snapshot.json");
    config.plasticity_state = root.join("plasticity_state.json");

    let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
        .expect("SessionState::initialize");

    // Allow writes to the tempdir.
    state.ingest_roots = vec![root.to_string_lossy().to_string()];
    state
}

fn make_test_file(dir: &Path) -> (PathBuf, String) {
    let path = dir.join("hello.py");
    let content = "def hello():\n    return 42\n";
    std::fs::write(&path, content).expect("write test file");
    (path, content.to_string())
}

// ===========================================================================
// Tests
// ===========================================================================

#[test]
fn test_preview_happy_path() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_test_state(dir.path());
    let (path, _old) = make_test_file(dir.path());

    let new_content = "def hello():\n    return 99\n".to_string();
    let out = handle_edit_preview(
        &mut state,
        EditPreviewInput {
            file_path: path.to_string_lossy().to_string(),
            agent_id: "test-agent".into(),
            new_content: new_content.clone(),
            description: Some("bump return value".into()),
        },
    )
    .expect("preview should succeed");

    assert!(out.preview_id.starts_with("preview_test-age"));
    assert!(out.snapshot.file_exists);
    assert_eq!(out.snapshot.line_count, 2);
    assert!(out.diff.lines_added > 0);
    assert!(out.validation.ready_to_commit);
    assert!(!out.validation.candidate_is_empty);
    assert!(!out.validation.candidate_equals_source);
    // Preview must NOT write to disk.
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert_ne!(on_disk, new_content);
}

#[test]
fn test_preview_nonexistent_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_test_state(dir.path());
    let ghost = dir.path().join("ghost.py");

    let out = handle_edit_preview(
        &mut state,
        EditPreviewInput {
            file_path: ghost.to_string_lossy().to_string(),
            agent_id: "test-agent".into(),
            new_content: "print('hi')\n".into(),
            description: None,
        },
    )
    .expect("preview of new file should succeed");

    assert!(!out.snapshot.file_exists);
    assert_eq!(out.snapshot.line_count, 0);
    assert!(out.diff.lines_added > 0);
    assert!(out.validation.ready_to_commit);
}

#[test]
fn test_commit_happy_path() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_test_state(dir.path());
    let (path, _old) = make_test_file(dir.path());

    let new_content = "def hello():\n    return 99\n".to_string();
    let preview = handle_edit_preview(
        &mut state,
        EditPreviewInput {
            file_path: path.to_string_lossy().to_string(),
            agent_id: "test-agent".into(),
            new_content: new_content.clone(),
            description: None,
        },
    )
    .unwrap();

    let commit = handle_edit_commit(
        &mut state,
        EditCommitInput {
            preview_id: preview.preview_id.clone(),
            agent_id: "test-agent".into(),
            confirm: true,
            reingest: false,
        },
    )
    .expect("commit should succeed");

    assert_eq!(commit.preview_id, preview.preview_id);
    assert!(commit.bytes_written > 0);
    // File must be updated on disk.
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert_eq!(on_disk, new_content);
    // Handle consumed — second commit must fail.
    assert!(state.edit_previews.get(&preview.preview_id).is_none());
}

#[test]
fn test_commit_handle_expired() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_test_state(dir.path());
    let (path, _) = make_test_file(dir.path());

    let preview = handle_edit_preview(
        &mut state,
        EditPreviewInput {
            file_path: path.to_string_lossy().to_string(),
            agent_id: "test-agent".into(),
            new_content: "expired test\n".into(),
            description: None,
        },
    )
    .unwrap();

    // Backdate the preview by 6 minutes.
    if let Some(entry) = state.edit_previews.get_mut(&preview.preview_id) {
        entry.created_at_ms = entry.created_at_ms.saturating_sub(6 * 60 * 1000);
    }

    let err = handle_edit_commit(
        &mut state,
        EditCommitInput {
            preview_id: preview.preview_id.clone(),
            agent_id: "test-agent".into(),
            confirm: true,
            reingest: false,
        },
    )
    .expect_err("expired preview should be rejected");

    let msg = format!("{}", err);
    assert!(
        msg.contains("not found") || msg.contains("expired"),
        "error should mention not found/expired, got: {msg}"
    );
    assert!(
        msg.contains("Hint:") && msg.contains("edit_preview"),
        "error should teach recovery via edit_preview, got: {msg}"
    );
}

#[test]
fn test_commit_source_modified() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_test_state(dir.path());
    let (path, _) = make_test_file(dir.path());

    let preview = handle_edit_preview(
        &mut state,
        EditPreviewInput {
            file_path: path.to_string_lossy().to_string(),
            agent_id: "test-agent".into(),
            new_content: "modified test\n".into(),
            description: None,
        },
    )
    .unwrap();

    // Tamper with the file on disk after preview.
    std::fs::write(&path, "def hello():\n    return 0\n").unwrap();

    let err = handle_edit_commit(
        &mut state,
        EditCommitInput {
            preview_id: preview.preview_id.clone(),
            agent_id: "test-agent".into(),
            confirm: true,
            reingest: false,
        },
    )
    .expect_err("source modification should be detected");

    let msg = format!("{}", err);
    assert!(
        msg.contains("source_modified"),
        "error should mention source_modified, got: {msg}"
    );
    assert!(
        msg.contains("Hint:") && msg.contains("edit_preview"),
        "error should explain that edit_preview must be rerun, got: {msg}"
    );
}

#[test]
fn test_commit_confirm_false() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_test_state(dir.path());
    let (path, _) = make_test_file(dir.path());

    let preview = handle_edit_preview(
        &mut state,
        EditPreviewInput {
            file_path: path.to_string_lossy().to_string(),
            agent_id: "test-agent".into(),
            new_content: "confirm test\n".into(),
            description: None,
        },
    )
    .unwrap();

    let err = handle_edit_commit(
        &mut state,
        EditCommitInput {
            preview_id: preview.preview_id.clone(),
            agent_id: "test-agent".into(),
            confirm: false,
            reingest: false,
        },
    )
    .expect_err("confirm=false should be rejected");

    let msg = format!("{}", err);
    assert!(
        msg.contains("confirm"),
        "error should mention confirm, got: {msg}"
    );
    assert!(
        msg.contains("Hint:") && msg.contains("Example:"),
        "error should explain how to retry with confirm=true, got: {msg}"
    );
}

#[test]
fn test_commit_handle_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = make_test_state(dir.path());

    let err = handle_edit_commit(
        &mut state,
        EditCommitInput {
            preview_id: "preview_bogus_0000".into(),
            agent_id: "test-agent".into(),
            confirm: true,
            reingest: false,
        },
    )
    .expect_err("bogus preview_id should fail");

    let msg = format!("{}", err);
    assert!(
        msg.contains("not found"),
        "error should mention not found, got: {msg}"
    );
    assert!(
        msg.contains("Hint:") && msg.contains("edit_preview"),
        "error should explain how to mint a fresh preview, got: {msg}"
    );
}
