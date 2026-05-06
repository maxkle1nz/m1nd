use std::path::Path;

pub(crate) fn normalize_path_text(value: &str) -> String {
    value.replace('\\', "/")
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
