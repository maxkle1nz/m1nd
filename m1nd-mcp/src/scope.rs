use m1nd_core::error::{M1ndError, M1ndResult};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// One existing path resolved beneath the immutable read roots captured by a
/// tool call. `root` is the canonical allow-root that authorized `path`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuthorizedExistingPath {
    pub path: PathBuf,
    pub root: PathBuf,
}

/// Canonicalize the configured workspace/ingest roots without widening them.
/// Invalid or disappearing roots never become authority.
pub(crate) fn canonical_read_roots(
    ingest_roots: &[String],
    workspace_root: Option<&str>,
) -> Vec<PathBuf> {
    let mut configured = ingest_roots.to_vec();
    if let Some(workspace) = workspace_root
        .map(str::trim)
        .filter(|root| !root.is_empty())
    {
        configured.push(workspace.to_string());
    }

    let mut seen = BTreeSet::new();
    let mut roots = Vec::new();
    for configured_root in configured {
        let Ok(canonical) = std::fs::canonicalize(&configured_root) else {
            continue;
        };
        let Ok(metadata) = std::fs::metadata(&canonical) else {
            continue;
        };
        if (metadata.is_dir() || metadata.is_file()) && seen.insert(canonical.clone()) {
            roots.push(canonical);
        }
    }
    roots
}

/// Resolve an existing path and prove that its canonical target is beneath an
/// already-authorized workspace/ingest root. A file root authorizes only that
/// exact file. Canonicalizing both sides makes `..` and symlinks that escape a
/// root fail closed.
pub(crate) fn authorize_existing_path(
    candidate: &Path,
    ingest_roots: &[String],
    workspace_root: Option<&str>,
    tool: &str,
) -> M1ndResult<AuthorizedExistingPath> {
    let canonical = std::fs::canonicalize(candidate).map_err(|error| {
        M1ndError::InvalidParams {
            tool: tool.to_string(),
            detail: format!(
                "path '{}' could not be canonicalized inside authorized workspace/ingest roots: {error}",
                candidate.display()
            ),
        }
    })?;

    let roots = canonical_read_roots(ingest_roots, workspace_root);
    if roots.is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: tool.to_string(),
            detail: format!(
                "path '{}' denied: no usable authorized workspace/ingest roots are configured",
                canonical.display()
            ),
        });
    }

    // Prefer the narrowest/deepest matching root so relative identities remain
    // stable when a workspace root contains a more specific ingest root.
    let mut selected: Option<PathBuf> = None;
    for root in roots {
        let root_is_file = std::fs::metadata(&root)
            .map(|metadata| metadata.is_file())
            .unwrap_or(false);
        let allowed = if root_is_file {
            canonical == root
        } else {
            canonical.starts_with(&root)
        };
        if allowed
            && selected
                .as_ref()
                .map(|current| root.components().count() > current.components().count())
                .unwrap_or(true)
        {
            selected = Some(root);
        }
    }

    let Some(root) = selected else {
        return Err(M1ndError::InvalidParams {
            tool: tool.to_string(),
            detail: format!(
                "path '{}' is outside authorized workspace/ingest roots",
                canonical.display()
            ),
        });
    };

    Ok(AuthorizedExistingPath {
        path: canonical,
        root,
    })
}

pub(crate) fn normalize_path_text(value: &str) -> String {
    let normalized = value.replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix("//?/UNC/") {
        return format!("//{rest}");
    }
    if let Some(rest) = normalized.strip_prefix("//?/") {
        return rest.to_string();
    }
    normalized
}

/// Normalize a scope-like path into the canonical repo-relative form.
///
/// Accepted inputs:
/// - `file::repo/path.rs`
/// - absolute paths under an ingest root
/// - relative repo paths
///
/// Returns `None` for empty input, repo-root scopes, or `file::` with no path.
pub fn normalize_scope_path(scope: Option<&str>, ingest_roots: &[String]) -> Option<String> {
    let scope = scope?.trim();
    if scope.is_empty() {
        return None;
    }

    let scope = scope.strip_prefix("file::").unwrap_or(scope);
    let scope = scope.strip_prefix("./").unwrap_or(scope);
    let scope = scope.strip_prefix(".\\").unwrap_or(scope);

    for root in ingest_roots {
        if let Some(rel) = strip_root_prefix_text(scope, root) {
            if rel.is_empty() || rel == "." {
                return None;
            }
            return Some(rel);
        }
    }

    let candidate = Path::new(scope);

    if candidate.is_absolute() {
        for root in ingest_roots {
            let root_path = Path::new(root);
            if let Some(rel) = strip_root_prefix(candidate, root_path) {
                if rel.is_empty() || rel == "." {
                    return None;
                }
                return Some(rel);
            }
        }

        let trimmed = candidate.to_string_lossy().trim_matches('/').to_string();
        return normalize_relative_scope(&trimmed);
    }

    normalize_relative_scope(&candidate.to_string_lossy())
}

fn strip_root_prefix(path: &Path, root: &Path) -> Option<String> {
    if let Ok(rel) = path.strip_prefix(root) {
        return Some(normalize_relative_text(&rel.to_string_lossy()));
    }

    if let Ok(root_canonical) = root.canonicalize() {
        if let Ok(path_canonical) = path.canonicalize() {
            if let Ok(rel) = path_canonical.strip_prefix(&root_canonical) {
                return Some(normalize_relative_text(&rel.to_string_lossy()));
            }
        }
    }

    None
}

fn strip_root_prefix_text(path: &str, root: &str) -> Option<String> {
    let path_norm = normalize_relative_text(path);
    let root_norm = normalize_relative_text(root);
    if path_norm.is_empty() || root_norm.is_empty() {
        return None;
    }

    let path_cmp;
    let root_cmp;
    #[cfg(windows)]
    {
        path_cmp = path_norm.to_ascii_lowercase();
        root_cmp = root_norm.to_ascii_lowercase();
    }
    #[cfg(not(windows))]
    {
        path_cmp = path_norm.clone();
        root_cmp = root_norm.clone();
    }

    if path_cmp == root_cmp {
        return Some(String::new());
    }

    let prefix = format!("{root_cmp}/");
    if path_cmp.starts_with(&prefix) {
        return Some(path_norm[root_norm.len() + 1..].to_string());
    }

    None
}

fn normalize_relative_text(scope: &str) -> String {
    normalize_path_text(scope)
        .trim()
        .trim_matches('/')
        .to_string()
}

fn normalize_relative_scope(scope: &str) -> Option<String> {
    let trimmed = normalize_relative_text(scope);
    if trimmed.is_empty() || trimmed == "." {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::{authorize_existing_path, normalize_path_text, normalize_scope_path};

    #[test]
    fn normalizes_windows_extended_path_prefixes() {
        assert_eq!(
            normalize_path_text(r"\\?\C:\repo\src\main.rs"),
            "C:/repo/src/main.rs"
        );
        assert_eq!(
            normalize_path_text(r"\\?\UNC\server\share\repo"),
            "//server/share/repo"
        );
    }

    #[test]
    fn normalizes_absolute_relative_and_file_prefix_scopes_to_the_same_path() {
        let roots = vec!["/workspace".to_string()];
        let abs = "/workspace/src/main.rs";

        assert_eq!(
            normalize_scope_path(Some(abs), &roots),
            Some("src/main.rs".to_string())
        );
        assert_eq!(
            normalize_scope_path(Some("src/main.rs"), &roots),
            Some("src/main.rs".to_string())
        );
        assert_eq!(
            normalize_scope_path(Some("file::src/main.rs"), &roots),
            Some("src/main.rs".to_string())
        );
    }

    #[test]
    fn rejects_empty_or_repo_root_scopes() {
        let roots = vec!["/workspace".to_string()];

        assert_eq!(normalize_scope_path(None, &roots), None);
        assert_eq!(normalize_scope_path(Some(""), &roots), None);
        assert_eq!(normalize_scope_path(Some("file::"), &roots), None);
        assert_eq!(normalize_scope_path(Some("/workspace"), &roots), None);
    }

    #[test]
    fn authorized_existing_path_accepts_only_canonical_targets_under_existing_roots() {
        let container = tempfile::tempdir().expect("container");
        let allowed = container.path().join("allowed");
        let outside = container.path().join("outside");
        std::fs::create_dir_all(&allowed).expect("allowed root");
        std::fs::create_dir_all(&outside).expect("outside root");
        let inside_file = allowed.join("inside.rs");
        let outside_file = outside.join("sentinel.rs");
        std::fs::write(&inside_file, "inside").expect("inside file");
        std::fs::write(&outside_file, "sentinel").expect("outside file");
        let roots = vec![allowed.to_string_lossy().into_owned()];

        let authorized = authorize_existing_path(&inside_file, &roots, None, "test")
            .expect("in-root file must be authorized");
        assert_eq!(
            authorized.path,
            std::fs::canonicalize(&inside_file).unwrap()
        );

        let denied = authorize_existing_path(&outside_file, &roots, None, "test")
            .expect_err("external file must be denied");
        assert!(denied.to_string().contains("outside authorized"));
    }

    #[cfg(unix)]
    #[test]
    fn authorized_existing_path_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let container = tempfile::tempdir().expect("container");
        let allowed = container.path().join("allowed");
        let outside = container.path().join("outside");
        std::fs::create_dir_all(&allowed).expect("allowed root");
        std::fs::create_dir_all(&outside).expect("outside root");
        let sentinel = outside.join("sentinel.rs");
        std::fs::write(&sentinel, "sentinel").expect("outside file");
        let escape = allowed.join("escape.rs");
        symlink(&sentinel, &escape).expect("escape symlink");
        let roots = vec![allowed.to_string_lossy().into_owned()];

        let denied = authorize_existing_path(&escape, &roots, None, "test")
            .expect_err("symlink resolving outside the root must be denied");
        assert!(denied.to_string().contains("outside authorized"));
    }
}
