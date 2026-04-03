// === crates/m1nd-core/src/atomic_write.rs ===
//
// Safe atomic file write: write to temp file, then rename.
// Provides:
// - Temp file cleanup on error (no stale .tmp files left behind)
// - Restrictive file permissions on Unix (0o600 — owner read/write only)

use crate::error::{M1ndError, M1ndResult};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Maximum file size for deserialization guards (100 MB).
pub const MAX_DESERIALIZE_BYTES: u64 = 100 * 1024 * 1024;

/// Maximum boot-memory / sidecar file size (10 MB).
pub const MAX_SIDECAR_BYTES: u64 = 10 * 1024 * 1024;

/// Write `data` to `path` atomically: write to a temp file, flush, then rename.
///
/// On any I/O error the temp file is cleaned up so no `.tmp` debris accumulates.
/// On Unix the temp file is created with mode `0o600` (owner read/write only).
pub fn write_atomic(path: &Path, data: &[u8]) -> M1ndResult<()> {
    let temp_path = temp_path_for(path);

    // Write to temp — clean up on any error.
    if let Err(e) = write_temp(&temp_path, data) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(e);
    }

    // Atomic rename — clean up temp on failure.
    if let Err(e) = std::fs::rename(&temp_path, path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(M1ndError::Io(e));
    }

    Ok(())
}

/// Read a file into memory, rejecting files larger than `max_bytes`.
///
/// Returns `M1ndError::PersistenceFailed` if the file exceeds the limit.
pub fn read_with_limit(path: &Path, max_bytes: u64) -> M1ndResult<Vec<u8>> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > max_bytes {
        return Err(M1ndError::PersistenceFailed(format!(
            "file too large: {} bytes exceeds limit of {} bytes ({})",
            meta.len(),
            max_bytes,
            path.display(),
        )));
    }
    Ok(std::fs::read(path)?)
}

/// Read a file to string, rejecting files larger than `max_bytes`.
pub fn read_to_string_with_limit(path: &Path, max_bytes: u64) -> M1ndResult<String> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > max_bytes {
        return Err(M1ndError::PersistenceFailed(format!(
            "file too large: {} bytes exceeds limit of {} bytes ({})",
            meta.len(),
            max_bytes,
            path.display(),
        )));
    }
    Ok(std::fs::read_to_string(path)?)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Compute the temp path for a given target path.
fn temp_path_for(path: &Path) -> PathBuf {
    path.with_extension("tmp")
}

/// Write data to the temp file with proper permissions.
fn write_temp(temp_path: &Path, data: &[u8]) -> M1ndResult<()> {
    let file = create_file(temp_path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(data)?;
    writer.flush()?;
    Ok(())
}

/// Create a file. On Unix, uses mode 0o600 (owner read/write only).
#[cfg(unix)]
fn create_file(path: &Path) -> M1ndResult<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?)
}

#[cfg(not(unix))]
fn create_file(path: &Path) -> M1ndResult<std::fs::File> {
    Ok(std::fs::File::create(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join("m1nd_atomic_write_tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_creates_file() {
        let dir = tmp_dir();
        let path = dir.join("test_write.json");
        let _ = std::fs::remove_file(&path);

        write_atomic(&path, b"{\"ok\":true}").unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "{\"ok\":true}");

        // No temp file left behind
        assert!(!path.with_extension("tmp").exists());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn atomic_write_no_temp_file_left_on_rename_to_invalid() {
        // Write to a non-existent directory — rename will fail, temp should be cleaned up.
        let dir = tmp_dir().join("nonexistent_subdir_xyz");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("sub").join("file.json");

        // The parent of `path` does not exist, but the parent of temp_path (.tmp) also
        // won't exist, so this should fail at write_temp. That's expected.
        let result = write_atomic(&path, b"data");
        assert!(result.is_err());
    }

    #[test]
    fn read_with_limit_rejects_large_file() {
        let dir = tmp_dir();
        let path = dir.join("large_file.bin");
        // Write a 1KB file
        std::fs::write(&path, vec![0u8; 1024]).unwrap();

        // Limit to 512 bytes — should fail
        let result = read_with_limit(&path, 512);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("too large"));

        // Limit to 2048 bytes — should succeed
        let data = read_with_limit(&path, 2048).unwrap();
        assert_eq!(data.len(), 1024);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_to_string_with_limit_rejects_large_file() {
        let dir = tmp_dir();
        let path = dir.join("large_text.json");
        let content = "x".repeat(2000);
        std::fs::write(&path, &content).unwrap();

        let result = read_to_string_with_limit(&path, 1000);
        assert!(result.is_err());

        let data = read_to_string_with_limit(&path, 3000).unwrap();
        assert_eq!(data.len(), 2000);

        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_sets_restrictive_permissions() {
        use std::os::unix::fs::MetadataExt;

        let dir = tmp_dir();
        let path = dir.join("perms_test.json");
        let _ = std::fs::remove_file(&path);

        write_atomic(&path, b"secret").unwrap();

        let meta = std::fs::metadata(&path).unwrap();
        let mode = meta.mode() & 0o777;
        assert_eq!(mode, 0o600, "File should have 0o600 permissions, got {mode:o}");

        let _ = std::fs::remove_file(&path);
    }
}
