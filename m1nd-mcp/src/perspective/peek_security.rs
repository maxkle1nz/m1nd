// === m1nd-mcp/src/perspective/peek_security.rs ===
// Theme 6: Peek Security Module.
// Standalone security pipeline executed on every peek call.
// Order: canonicalize → allow-list → existence → staleness → size → binary → encoding → truncation → wrap.

use m1nd_core::error::{M1ndError, M1ndResult};
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::ffi::CString;
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use super::state::{PeekContent, PeekSecurityConfig};

// ---------------------------------------------------------------------------
// Security pipeline
// ---------------------------------------------------------------------------

/// Execute the full peek security pipeline.
///
/// Steps (Theme 6, in order):
/// 1. Path canonicalization
/// 2. Allow-list validation (reject symlinks resolving outside)
/// 3. File existence check
/// 4. Staleness detection (compare mtime against last ingest)
/// 5. File size cap (reject > 10MB)
/// 6. Binary detection (NUL in first 8KB)
/// 7. Encoding safety (from_utf8_lossy)
/// 8. Truncation on char boundaries (max_chars)
/// 9. Content wrapping (structural markers for prompt injection defense)
pub fn secure_peek(
    source_path: &str,
    config: &PeekSecurityConfig,
    line_hint: Option<u32>,
    last_ingest_ms: Option<u64>,
) -> M1ndResult<PeekContent> {
    // Steps 1-3: resolve the candidate, select an allow root, then open exactly
    // once through that root descriptor. Every component is O_NOFOLLOW on Unix,
    // so a same-UID rename/symlink swap cannot redirect the later metadata/read.
    let canonical = canonicalize_path(source_path)?;
    let canonical_root = select_allow_root(&canonical, &config.allow_roots)?;
    let mut file = open_beneath_root(&canonical_root, &canonical)?;

    // Step 4: Staleness
    let metadata = file.metadata().map_err(M1ndError::Io)?;
    let provenance_stale = check_staleness_metadata(&metadata, last_ingest_ms);

    // Step 5: File size
    if !metadata.is_file() {
        return Err(M1ndError::InvalidParams {
            tool: "perspective.peek".into(),
            detail: format!("source is not a regular file: {}", canonical.display()),
        });
    }
    if metadata.len() > config.max_file_size {
        return Err(M1ndError::InvalidParams {
            tool: "perspective.peek".into(),
            detail: format!(
                "file too large: {} bytes (cap: {})",
                metadata.len(),
                config.max_file_size
            ),
        });
    }

    // Step 6: Binary detection
    check_binary_and_rewind(&mut file, &canonical)?;

    // Step 7+8: Read with encoding safety and line-bounded extraction
    let center_line = line_hint.unwrap_or(1);
    let start_line = center_line.saturating_sub(config.max_lines_before);
    let end_line = center_line + config.max_lines_after;

    let (content, actual_start, actual_end, encoding_lossy) =
        extract_lines_from_file(file, start_line, end_line, config.max_chars)?;

    // Step 9: Relative path
    let relative_path = canonical
        .strip_prefix(&canonical_root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| canonical.display().to_string());

    Ok(PeekContent {
        content,
        truncated: actual_end < end_line, // simplified: true if we stopped before end_line
        provenance_stale,
        encoding_lossy,
        relative_path,
        line_start: actual_start,
        line_end: actual_end,
    })
}

// ---------------------------------------------------------------------------
// Internal pipeline steps
// ---------------------------------------------------------------------------

fn canonicalize_path(path: &str) -> M1ndResult<PathBuf> {
    fs::canonicalize(path).map_err(|e| M1ndError::InvalidParams {
        tool: "perspective.peek".into(),
        detail: format!("path canonicalization failed for '{}': {}", path, e),
    })
}

fn select_allow_root(canonical: &Path, allow_roots: &[String]) -> M1ndResult<PathBuf> {
    if allow_roots.is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: "perspective.peek".into(),
            detail: format!(
                "path '{}' denied: no allowed ingest roots are configured",
                canonical.display()
            ),
        });
    }

    for root in allow_roots {
        let root_path = PathBuf::from(root);
        if let Ok(canonical_root) = fs::canonicalize(&root_path) {
            if canonical.starts_with(&canonical_root) {
                return Ok(canonical_root);
            }
        }
    }

    Err(M1ndError::InvalidParams {
        tool: "perspective.peek".into(),
        detail: format!("path '{}' is outside allowed roots", canonical.display()),
    })
}

#[cfg(unix)]
fn c_path(path: &Path) -> M1ndResult<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| M1ndError::InvalidParams {
        tool: "perspective.peek".into(),
        detail: format!("path contains a NUL byte: {}", path.display()),
    })
}

#[cfg(unix)]
fn open_no_follow(path: &Path, directory: bool) -> M1ndResult<fs::File> {
    let path_c = c_path(path)?;
    let mut flags = libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW;
    if directory {
        flags |= libc::O_DIRECTORY;
    }
    // SAFETY: `path_c` is a valid NUL-terminated path and the returned fd is
    // immediately owned by File on success.
    let fd = unsafe { libc::open(path_c.as_ptr(), flags) };
    if fd < 0 {
        return Err(M1ndError::Io(std::io::Error::last_os_error()));
    }
    // SAFETY: `fd` is newly returned by open and has no other Rust owner.
    Ok(unsafe { fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn openat_no_follow(
    parent: &fs::File,
    component: &std::ffi::OsStr,
    directory: bool,
) -> M1ndResult<fs::File> {
    let component_c = CString::new(component.as_bytes()).map_err(|_| M1ndError::InvalidParams {
        tool: "perspective.peek".into(),
        detail: "path component contains a NUL byte".into(),
    })?;
    let mut flags = libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW;
    if directory {
        flags |= libc::O_DIRECTORY;
    }
    // SAFETY: parent is a live directory fd and component_c is NUL-terminated.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), component_c.as_ptr(), flags) };
    if fd < 0 {
        return Err(M1ndError::Io(std::io::Error::last_os_error()));
    }
    // SAFETY: `fd` is newly returned by openat and has no other Rust owner.
    Ok(unsafe { fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn same_file_identity(expected: &fs::Metadata, observed: &fs::Metadata) -> bool {
    expected.dev() == observed.dev() && expected.ino() == observed.ino()
}

#[cfg(unix)]
fn open_beneath_root(canonical_root: &Path, canonical: &Path) -> M1ndResult<fs::File> {
    let root_metadata = fs::metadata(canonical_root).map_err(M1ndError::Io)?;
    if root_metadata.is_file() {
        if canonical != canonical_root {
            return Err(M1ndError::InvalidParams {
                tool: "perspective.peek".into(),
                detail: "a file allow-root only authorizes that exact file".into(),
            });
        }
        let file = open_no_follow(canonical_root, false)?;
        if !same_file_identity(&root_metadata, &file.metadata().map_err(M1ndError::Io)?) {
            return Err(M1ndError::InvalidParams {
                tool: "perspective.peek".into(),
                detail: "allow-root changed while it was being opened".into(),
            });
        }
        return Ok(file);
    }
    if !root_metadata.is_dir() {
        return Err(M1ndError::InvalidParams {
            tool: "perspective.peek".into(),
            detail: "allow-root is neither a regular file nor directory".into(),
        });
    }

    let relative =
        canonical
            .strip_prefix(canonical_root)
            .map_err(|_| M1ndError::InvalidParams {
                tool: "perspective.peek".into(),
                detail: format!("path '{}' is outside allowed root", canonical.display()),
            })?;
    let components = relative.components().collect::<Vec<_>>();
    if components.is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: "perspective.peek".into(),
            detail: "a directory allow-root is not itself readable as a file".into(),
        });
    }

    let mut current = open_no_follow(canonical_root, true)?;
    if !same_file_identity(&root_metadata, &current.metadata().map_err(M1ndError::Io)?) {
        return Err(M1ndError::InvalidParams {
            tool: "perspective.peek".into(),
            detail: "allow-root changed while it was being opened".into(),
        });
    }
    for (index, component) in components.iter().enumerate() {
        let name = match component {
            std::path::Component::Normal(name) => *name,
            _ => {
                return Err(M1ndError::InvalidParams {
                    tool: "perspective.peek".into(),
                    detail: "canonical relative path contains a non-normal component".into(),
                });
            }
        };
        let last = index + 1 == components.len();
        let next = openat_no_follow(&current, name, !last)?;
        if last {
            return Ok(next);
        }
        current = next;
    }
    unreachable!("non-empty component list returns on its final component")
}

#[cfg(not(unix))]
fn open_beneath_root(_canonical_root: &Path, _canonical: &Path) -> M1ndResult<fs::File> {
    Err(M1ndError::InvalidParams {
        tool: "perspective.peek".into(),
        detail: "safe descriptor-anchored Peek is not yet implemented on this platform".into(),
    })
}

fn check_staleness_metadata(metadata: &fs::Metadata, last_ingest_ms: Option<u64>) -> bool {
    let last_ingest = match last_ingest_ms {
        Some(ms) => ms,
        None => return false, // no ingest info — can't determine staleness
    };

    let mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    mtime > last_ingest
}

fn check_binary_and_rewind(file: &mut fs::File, canonical: &Path) -> M1ndResult<()> {
    let mut buf = [0u8; 8192];
    let n = file.read(&mut buf).map_err(M1ndError::Io)?;

    if buf[..n].contains(&0) {
        return Err(M1ndError::InvalidParams {
            tool: "perspective.peek".into(),
            detail: format!("binary content detected in '{}'", canonical.display()),
        });
    }

    file.seek(SeekFrom::Start(0)).map_err(M1ndError::Io)?;
    Ok(())
}

fn extract_lines_from_file(
    file: fs::File,
    start_line: u32,
    end_line: u32,
    max_chars: usize,
) -> M1ndResult<(String, u32, u32, bool)> {
    let reader = BufReader::new(file);

    let mut result = String::new();
    let mut actual_start = start_line;
    let mut actual_end = start_line;
    let mut encoding_lossy = false;
    let mut char_count = 0;
    let mut started = false;

    for (line_num_0, line_result) in reader.split(b'\n').enumerate() {
        let line_num = (line_num_0 + 1) as u32;

        if line_num < start_line {
            continue;
        }
        if line_num > end_line {
            break;
        }

        let raw_bytes = line_result.map_err(M1ndError::Io)?;
        let line_str = String::from_utf8_lossy(&raw_bytes);

        if line_str.as_ref() != std::str::from_utf8(&raw_bytes).unwrap_or("") {
            encoding_lossy = true;
        }

        if !started {
            actual_start = line_num;
            started = true;
        }

        let remaining = max_chars.saturating_sub(char_count);
        if remaining == 0 {
            break;
        }

        // Truncate on char boundaries
        let truncated_line: String = line_str.chars().take(remaining).collect();
        char_count += truncated_line.len();

        if !result.is_empty() {
            result.push('\n');
            char_count += 1;
        }
        result.push_str(&truncated_line);
        actual_end = line_num;

        if char_count >= max_chars {
            break;
        }
    }

    Ok((result, actual_start, actual_end, encoding_lossy))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn canonicalize_rejects_nonexistent() {
        let result = canonicalize_path("/nonexistent/path/foo.rs");
        assert!(result.is_err());
    }

    #[test]
    fn empty_allow_list_denies_every_path() {
        let f = temp_file("private");
        let source_path = f.path().to_string_lossy().into_owned();

        let err = secure_peek(&source_path, &PeekSecurityConfig::default(), None, None)
            .expect_err("an empty allow-list must fail closed for every path");
        assert!(
            err.to_string().contains("no allowed ingest roots"),
            "got: {err}"
        );
    }

    #[test]
    fn allow_list_accepts_a_file_inside_a_canonical_root() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("inside.rs");
        fs::write(&source, "fn inside() {}\n").unwrap();
        let source_path = source.to_string_lossy().into_owned();
        let config = PeekSecurityConfig {
            allow_roots: vec![root.path().to_string_lossy().into_owned()],
            ..PeekSecurityConfig::default()
        };

        let content = secure_peek(&source_path, &config, None, None)
            .expect("a file inside an allowed canonical root must be readable");
        assert!(content.content.contains("fn inside()"));
    }

    #[test]
    fn allow_list_denies_a_file_outside_every_canonical_root() {
        let allowed = tempfile::tempdir().unwrap();
        let outside = temp_file("outside");
        let source_path = outside.path().to_string_lossy().into_owned();
        let config = PeekSecurityConfig {
            allow_roots: vec![allowed.path().to_string_lossy().into_owned()],
            ..PeekSecurityConfig::default()
        };

        let err = secure_peek(&source_path, &config, None, None)
            .expect_err("a file outside every allowed root must be denied");
        assert!(
            err.to_string().contains("outside allowed roots"),
            "got: {err}"
        );
    }

    #[test]
    fn bidi_normalize_order() {
        let (lo, hi) = super::super::keys::normalize_bidi_endpoints("z", "a");
        assert_eq!(lo, "a");
        assert_eq!(hi, "z");
    }

    #[test]
    fn binary_detection_catches_nul() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"hello\x00world").unwrap();
        f.flush().unwrap();
        let canonical = fs::canonicalize(f.path()).unwrap();
        let mut file = open_beneath_root(&canonical, &canonical).unwrap();
        let result = check_binary_and_rewind(&mut file, &canonical);
        assert!(result.is_err());
    }

    #[test]
    fn extract_lines_respects_range() {
        let content = "line1\nline2\nline3\nline4\nline5\n";
        let f = temp_file(content);
        let canonical = fs::canonicalize(f.path()).unwrap();
        let file = open_beneath_root(&canonical, &canonical).unwrap();
        let (text, start, end, _lossy) = extract_lines_from_file(file, 2, 4, 2000).unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 4);
        assert!(text.contains("line2"));
        assert!(text.contains("line4"));
        assert!(!text.contains("line1"));
    }

    #[test]
    fn extract_lines_respects_char_cap() {
        let content = "a".repeat(100) + "\n" + &"b".repeat(100);
        let f = temp_file(&content);
        let canonical = fs::canonicalize(f.path()).unwrap();
        let file = open_beneath_root(&canonical, &canonical).unwrap();
        let (text, _, _, _) = extract_lines_from_file(file, 1, 10, 50).unwrap();
        assert!(text.len() <= 51); // 50 chars + possible newline
    }

    #[test]
    fn staleness_detection() {
        let f = temp_file("fresh");
        let canonical = fs::canonicalize(f.path()).unwrap();
        let metadata = fs::metadata(canonical).unwrap();
        // File modified "now" vs last_ingest far in the past
        assert!(check_staleness_metadata(&metadata, Some(1)));
        // No ingest info
        assert!(!check_staleness_metadata(&metadata, None));
    }

    #[cfg(unix)]
    #[test]
    fn descriptor_walk_refuses_an_ancestor_symlink_swap() {
        use std::os::unix::fs::symlink;

        let container = tempfile::tempdir().unwrap();
        let root = container.path().join("root");
        let inside_dir = root.join("nested");
        let outside_dir = container.path().join("outside");
        fs::create_dir_all(&inside_dir).unwrap();
        fs::create_dir_all(&outside_dir).unwrap();
        let source = inside_dir.join("target.rs");
        fs::write(&source, "inside").unwrap();
        fs::write(outside_dir.join("target.rs"), "outside").unwrap();

        let canonical_root = fs::canonicalize(&root).unwrap();
        let canonical_source = fs::canonicalize(&source).unwrap();
        fs::rename(&inside_dir, root.join("nested-original")).unwrap();
        symlink(&outside_dir, &inside_dir).unwrap();

        let error = open_beneath_root(&canonical_root, &canonical_source)
            .expect_err("an ancestor swapped to a symlink must never be followed");
        assert!(matches!(error, M1ndError::Io(_)), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn descriptor_walk_refuses_a_final_symlink_swap() {
        use std::os::unix::fs::symlink;

        let container = tempfile::tempdir().unwrap();
        let root = container.path().join("root");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("target.rs");
        let outside = container.path().join("outside.rs");
        fs::write(&source, "inside").unwrap();
        fs::write(&outside, "outside").unwrap();

        let canonical_root = fs::canonicalize(&root).unwrap();
        let canonical_source = fs::canonicalize(&source).unwrap();
        fs::remove_file(&source).unwrap();
        symlink(&outside, &source).unwrap();

        let error = open_beneath_root(&canonical_root, &canonical_source)
            .expect_err("a final component swapped to a symlink must never be followed");
        assert!(matches!(error, M1ndError::Io(_)), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn metadata_binary_check_and_content_use_the_same_open_file() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("target.rs");
        fs::write(&source, "original\n").unwrap();
        let canonical_root = fs::canonicalize(root.path()).unwrap();
        let canonical_source = fs::canonicalize(&source).unwrap();
        let mut file = open_beneath_root(&canonical_root, &canonical_source).unwrap();

        fs::rename(&source, root.path().join("original.rs")).unwrap();
        fs::write(&source, "replacement\n").unwrap();

        check_binary_and_rewind(&mut file, &canonical_source).unwrap();
        let (content, _, _, _) = extract_lines_from_file(file, 1, 2, 100).unwrap();
        assert_eq!(content, "original");
    }
}
