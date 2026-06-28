use std::path::Path;

pub const NOISE_DIR_NAMES: &[&str] = &[
    ".cache",
    ".git",
    ".hg",
    ".m1nd-benchmark-fixtures",
    ".m1nd-field-workspaces",
    ".m1nd-real-audit-runtime",
    ".m1nd-self-audit-runtime",
    ".mypy_cache",
    ".next",
    ".pytest_cache",
    ".ruff_cache",
    ".svn",
    ".venv",
    ".vault",
    ".roomanizer",
    "__pycache__",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "target",
    "vendor",
    "venv",
    "wiki-build",
];

pub const RUNTIME_ARTIFACT_FILE_NAMES: &[&str] = &[
    "alerts.json",
    "antibodies.json",
    "auto_ingest_events.jsonl",
    "auto_ingest_state.json",
    "global_savings.json",
    "graph_snapshot.json",
    "ingest_roots.json",
    "plasticity_state.json",
    "trust_state.json",
    "tremor_state.json",
];

pub fn default_skip_dirs() -> Vec<String> {
    NOISE_DIR_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect()
}

pub fn is_noise_dir_name(name: &str) -> bool {
    NOISE_DIR_NAMES.contains(&name)
}

pub fn is_runtime_artifact_file_name(name: &str) -> bool {
    RUNTIME_ARTIFACT_FILE_NAMES.contains(&name)
}

pub fn is_editor_temp_file_name(name: &str) -> bool {
    name.ends_with('~')
        || name.ends_with(".swp")
        || name.ends_with(".tmp")
        || name == ".DS_Store"
        || name.starts_with(".#")
        || name.starts_with("4913")
}

pub fn is_noise_dir_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(is_noise_dir_name)
}

pub fn is_noise_path(path: &Path) -> bool {
    if path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(is_noise_dir_name)
    }) {
        return true;
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| is_editor_temp_file_name(name) || is_runtime_artifact_file_name(name))
}

/// Parse simple top-level directory names from `.gitignore` text.
/// Honours the common big-noise case (e.g. `donors/`, `dist/`) without pulling
/// a full gitignore engine: keeps plain dir names, drops comments, negations
/// (`!`), globs, and nested paths. Names are matched by component during the walk.
pub fn parse_gitignore_dir_names(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let s = line.trim_end_matches('/').trim_start_matches('/');
        if s.is_empty() || s.contains('/') || s.contains('*') || s.contains('?') || s.contains('[') {
            continue;
        }
        if !out.iter().any(|e| e == s) {
            out.push(s.to_string());
        }
    }
    out
}

/// Read `<root>/.gitignore` and return simple dir names to skip during ingest.
/// Missing/unreadable `.gitignore` → empty (no-op).
pub fn gitignore_skip_dir_names(root: &Path) -> Vec<String> {
    match std::fs::read_to_string(root.join(".gitignore")) {
        Ok(text) => parse_gitignore_dir_names(&text),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_noise_directories_anywhere_in_path() {
        assert!(is_noise_path(Path::new("/repo/.venv/lib/site.py")));
        assert!(is_noise_path(Path::new(
            "/repo/.m1nd-field-workspaces/round/repo/file.rs"
        )));
        assert!(is_noise_dir_path(Path::new("/repo/node_modules")));
        assert!(!is_noise_path(Path::new("/repo/src/lib.rs")));
    }

    #[test]
    fn detects_runtime_artifacts_and_editor_temp_files() {
        assert!(is_noise_path(Path::new("/repo/graph_snapshot.json")));
        assert!(is_noise_path(Path::new("/repo/plasticity_state.json")));
        assert!(is_noise_path(Path::new("/repo/file.md.swp")));
        assert!(is_noise_path(Path::new("/repo/.DS_Store")));
        assert!(!is_noise_path(Path::new("/repo/docs/notes.md")));
    }

    #[test]
    fn parses_simple_gitignore_dir_names_only() {
        let gi = "donors\n/build/\n# comment\n!keep\n*.log\nsub/dir\ndist/\ndonors\n";
        assert_eq!(
            parse_gitignore_dir_names(gi),
            vec!["donors".to_string(), "build".to_string(), "dist".to_string()]
        );
    }
}
