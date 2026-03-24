use std::path::Path;

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
        return Some(rel.to_string_lossy().trim_matches('/').to_string());
    }

    if let Ok(root_canonical) = root.canonicalize() {
        if let Ok(path_canonical) = path.canonicalize() {
            if let Ok(rel) = path_canonical.strip_prefix(&root_canonical) {
                return Some(rel.to_string_lossy().trim_matches('/').to_string());
            }
        }
    }

    None
}

fn normalize_relative_scope(scope: &str) -> Option<String> {
    let trimmed = scope.trim().trim_matches('/').to_string();
    if trimmed.is_empty() || trimmed == "." {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_scope_path;

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
}
