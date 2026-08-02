// === crates/m1nd-ingest/src/walker.rs ===

use crate::path_policy::{is_noise_dir_name, is_noise_path};
use m1nd_core::error::{M1ndError, M1ndResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

trait DiscoveryProbe {
    fn is_binary(&self, path: &Path) -> M1ndResult<bool>;
    fn path_metadata(&self, path: &Path) -> std::io::Result<std::fs::Metadata>;
    fn entry_metadata(&self, entry: &ignore::DirEntry) -> Result<std::fs::Metadata, ignore::Error>;
}

struct OsDiscoveryProbe;

impl DiscoveryProbe for OsDiscoveryProbe {
    fn is_binary(&self, path: &Path) -> M1ndResult<bool> {
        DirectoryWalker::is_binary(path)
    }

    fn path_metadata(&self, path: &Path) -> std::io::Result<std::fs::Metadata> {
        path.metadata()
    }

    fn entry_metadata(&self, entry: &ignore::DirEntry) -> Result<std::fs::Metadata, ignore::Error> {
        entry.metadata()
    }
}

// ---------------------------------------------------------------------------
// DirectoryWalker — file discovery
// FM-ING-004 fix: binary detection (NUL byte in first 8KB).
// Replaces: ingest.py CodebaseIngestor._walk_directory()
// ---------------------------------------------------------------------------

/// A discovered file with metadata.
#[derive(Clone, Debug)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    pub relative_path: String,
    pub extension: Option<String>,
    pub size_bytes: u64,
    pub last_modified: f64,
    /// Number of git commits touching this file (0 if not in a git repo).
    pub commit_count: u32,
    /// Timestamp of most recent git commit for this file (0.0 if unavailable).
    pub last_commit_time: f64,
}

/// Result of directory walking including co-change commit groups.
#[derive(Clone, Debug, Default)]
pub struct WalkResult {
    pub files: Vec<DiscoveredFile>,
    /// Each inner Vec is a group of relative_paths that changed together in one commit.
    pub commit_groups: Vec<Vec<String>>,
    /// Exact, fail-closed identity of the VCS producer/context used for temporal
    /// enrichment. `unversioned` is an explicit observation, never a fallback
    /// for a failed Git command.
    pub vcs: VcsContextV1,
}

pub const VCS_CONTEXT_SCHEMA_V1: &str = "m1nd-vcs-context-v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VcsContextV1 {
    pub schema: String,
    pub kind: String,
    pub git_version: Option<String>,
    pub repository_root: Option<String>,
    pub git_dir: Option<String>,
    pub head_oid: Option<String>,
    pub history_output_digest: Option<String>,
}

impl Default for VcsContextV1 {
    fn default() -> Self {
        Self {
            schema: VCS_CONTEXT_SCHEMA_V1.into(),
            kind: "unversioned".into(),
            git_version: None,
            repository_root: None,
            git_dir: None,
            head_oid: None,
            history_output_digest: None,
        }
    }
}

impl VcsContextV1 {
    pub fn is_git(&self) -> bool {
        self.kind == "git"
    }

    pub fn valid(&self) -> bool {
        if self.schema != VCS_CONTEXT_SCHEMA_V1 {
            return false;
        }
        match self.kind.as_str() {
            "unversioned" => {
                self.git_version.is_none()
                    && self.repository_root.is_none()
                    && self.git_dir.is_none()
                    && self.head_oid.is_none()
                    && self.history_output_digest.is_none()
            }
            "git" => {
                self.git_version
                    .as_ref()
                    .is_some_and(|value| !value.is_empty() && value == value.trim())
                    && self
                        .repository_root
                        .as_ref()
                        .is_some_and(|value| Path::new(value).is_absolute())
                    && self
                        .git_dir
                        .as_ref()
                        .is_some_and(|value| Path::new(value).is_absolute())
                    && self.history_output_digest.as_ref().is_some_and(|value| {
                        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
                    })
                    && self
                        .head_oid
                        .as_ref()
                        .map(|value| {
                            (value.len() == 40 || value.len() == 64)
                                && value.bytes().all(|byte| byte.is_ascii_hexdigit())
                        })
                        .unwrap_or(true)
            }
            _ => false,
        }
    }
}

/// Directory walker with skip rules and binary file detection.
/// Replaces: ingest.py SKIP_DIRS, SKIP_FILES, and walk logic
pub struct DirectoryWalker {
    skip_dirs: Vec<String>,
    skip_files: Vec<String>,
    include_dotfiles: bool,
    dotfile_patterns: Vec<String>,
}

impl DirectoryWalker {
    pub fn new(
        skip_dirs: Vec<String>,
        skip_files: Vec<String>,
        include_dotfiles: bool,
        dotfile_patterns: Vec<String>,
    ) -> Self {
        Self {
            skip_dirs,
            skip_files,
            include_dotfiles,
            dotfile_patterns,
        }
    }

    fn normalize_rel_path(path: &Path, root: &Path) -> String {
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        #[cfg(windows)]
        return relative.replace('\\', "/");
        #[cfg(not(windows))]
        relative
    }

    fn validate_rel_path(relative_path: &str) -> M1ndResult<()> {
        if crate::is_valid_relative_file_path(relative_path) {
            Ok(())
        } else {
            Err(M1ndError::InvalidParams {
                tool: "directory_walk".into(),
                detail: format!("walker discovered non-bijective relative path {relative_path:?}"),
            })
        }
    }

    fn dotfile_pattern_matches(pattern: &str, rel_path: &str) -> bool {
        let pattern = pattern.trim().trim_start_matches("./");
        let rel_path = rel_path.trim_start_matches("./").trim_matches('/');
        if pattern.is_empty() {
            return false;
        }
        if let Some(prefix) = pattern.strip_suffix("/**") {
            return rel_path == prefix || rel_path.starts_with(&format!("{}/", prefix));
        }
        if let Some(prefix) = pattern.strip_suffix("/*") {
            return rel_path == prefix || rel_path.starts_with(&format!("{}/", prefix));
        }
        rel_path == pattern
    }

    fn allow_hidden_path(&self, rel_path: &str) -> bool {
        self.include_dotfiles
            && self
                .dotfile_patterns
                .iter()
                .any(|pattern| Self::dotfile_pattern_matches(pattern, rel_path))
    }

    /// Walk directory and return all non-binary, non-skipped files.
    /// FM-ING-004 fix: checks first 8KB for NUL bytes to detect binary files.
    /// Traversal, binary-probe, and metadata I/O failures abort the governed
    /// discovery; only explicit policy exclusions and proven binary files skip.
    /// Replaces: ingest.py directory walking logic
    pub fn walk(&self, root: &Path) -> M1ndResult<WalkResult> {
        self.walk_with_probe(root, &OsDiscoveryProbe)
    }

    fn walk_with_probe<P: DiscoveryProbe>(&self, root: &Path, probe: &P) -> M1ndResult<WalkResult> {
        use ignore::WalkBuilder;

        if !root.exists() {
            return Err(M1ndError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Root directory not found: {}", root.display()),
            )));
        }

        let mut files = Vec::new();
        let root_canonical = root.canonicalize().map_err(M1ndError::Io)?;

        if root_canonical.is_file() {
            let rel_path = root_canonical
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
                .unwrap_or_else(|| root_canonical.to_string_lossy().replace('\\', "/"));
            if !rel_path.is_empty() {
                Self::validate_rel_path(&rel_path)?;
                match probe.is_binary(&root_canonical) {
                    Ok(true) => {}
                    Ok(false) => {
                        let metadata = probe
                            .path_metadata(&root_canonical)
                            .map_err(M1ndError::Io)?;
                        let last_modified = metadata
                            .modified()
                            .map_err(M1ndError::Io)?
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|duration| duration.as_secs_f64())
                            .unwrap_or(0.0);
                        files.push(DiscoveredFile {
                            path: root_canonical.clone(),
                            relative_path: rel_path,
                            extension: root_canonical
                                .extension()
                                .map(|ext| ext.to_string_lossy().to_string()),
                            size_bytes: metadata.len(),
                            last_modified,
                            commit_count: 0,
                            last_commit_time: 0.0,
                        });
                    }
                    Err(error) => return Err(error),
                }
            }

            let vcs_root = root_canonical.parent().unwrap_or(&root_canonical);
            let (commit_groups, vcs) = Self::enrich_with_git(vcs_root, &mut files)?;
            return Ok(WalkResult {
                files,
                commit_groups,
                vcs,
            });
        }

        // Gitignore-aware walk (ignore crate — same walker ripgrep uses).
        // standard_filters default keeps git_ignore/git_global/git_exclude/ignore/parents ON.
        // hidden(!include_dotfiles) mirrors the old hidden-dir/file pruning while honoring the
        // include_dotfiles escape hatch. The filter_entry below preserves the hardcoded
        // skip_dirs + is_noise_dir_name pruning so noise dirs are dropped even in a repo with
        // NO .gitignore.
        // An unreadable SCAN ROOT is fatal (nothing at all can be discovered,
        // and an empty walk would silently claim an empty repo). An unreadable
        // entry DEEPER in the tree is skipped below — it can contain nothing
        // ingestable, and a whole birth ceremony once died on a foreign-owned
        // `.secrets/` directory (2026-08-02).
        std::fs::read_dir(&root_canonical).map_err(|error| {
            M1ndError::Io(std::io::Error::other(format!(
                "directory walk cannot open the scan root {}: {error}",
                root_canonical.display()
            )))
        })?;
        let filter_root = root_canonical.clone();
        let skip_dirs = self.skip_dirs.clone();
        let include_dotfiles = self.include_dotfiles;
        let dotfile_patterns = self.dotfile_patterns.clone();
        let walk = WalkBuilder::new(&root_canonical)
            .follow_links(false)
            .hidden(!self.include_dotfiles)
            .filter_entry(move |e| {
                if e.file_type().is_some_and(|ft| ft.is_dir()) {
                    let name = e.file_name().to_string_lossy();
                    let rel_path = Self::normalize_rel_path(e.path(), &filter_root);
                    if is_noise_dir_name(&name) {
                        return false;
                    }
                    // Skip hidden dirs and configured skip dirs (mirrors old filter_entry).
                    let allow_hidden = include_dotfiles
                        && dotfile_patterns
                            .iter()
                            .any(|pattern| Self::dotfile_pattern_matches(pattern, &rel_path));
                    if name.starts_with('.') && name != "." && !allow_hidden {
                        return false;
                    }
                    return !skip_dirs.iter().any(|s| name == s.as_str());
                }
                true
            })
            .build();

        for entry in walk {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    // The root was proven readable above; anything failing
                    // here is a deeper unreadable path, which can contain
                    // nothing ingestable. Skip it loudly instead of killing
                    // the whole discovery.
                    eprintln!(
                        "[m1nd ingest] skipping unreadable path under {}: {error}",
                        root_canonical.display()
                    );
                    continue;
                }
            };

            // ignore::DirEntry::file_type is Option (None for stdin/errors) — skip non-files.
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            let file_name = entry.file_name().to_string_lossy();

            if is_noise_path(entry.path()) {
                continue;
            }

            // Skip configured file patterns
            if self.skip_files.iter().any(|s| file_name == s.as_str()) {
                continue;
            }

            let rel_path = Self::normalize_rel_path(entry.path(), &root_canonical);
            Self::validate_rel_path(&rel_path)?;

            // Skip hidden files unless explicitly allowed.
            if file_name.starts_with('.') && !self.allow_hidden_path(&rel_path) {
                continue;
            }

            let path = entry.path().to_path_buf();

            // FM-ING-004: skip binary files
            match probe.is_binary(&path) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => return Err(error),
            }

            let metadata = match probe.entry_metadata(&entry) {
                Ok(m) => m,
                Err(error) => {
                    return Err(M1ndError::Io(std::io::Error::other(format!(
                        "metadata read failed for {} under {}: {error}",
                        path.display(),
                        root_canonical.display()
                    ))));
                }
            };

            let relative_path = rel_path;

            let extension = path.extension().map(|e| e.to_string_lossy().to_string());

            let last_modified = metadata
                .modified()
                .map_err(M1ndError::Io)?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);

            files.push(DiscoveredFile {
                path,
                relative_path,
                extension,
                size_bytes: metadata.len(),
                last_modified,
                commit_count: 0,
                last_commit_time: 0.0,
            });
        }

        // Enrich with git history if available, and collect commit groups
        let (commit_groups, vcs) = Self::enrich_with_git(&root_canonical, &mut files)?;

        Ok(WalkResult {
            files,
            commit_groups,
            vcs,
        })
    }

    /// Enrich discovered files with git history (commit count + last commit time).
    /// A root with no `.git` marker is explicitly `unversioned`. Once a marker
    /// exists, every Git discovery/parse failure is fatal: a broken repository
    /// must never be represented by the same neutral zeros as a real non-repo.
    fn enrich_with_git(
        root: &Path,
        files: &mut [DiscoveredFile],
    ) -> M1ndResult<(Vec<Vec<String>>, VcsContextV1)> {
        use std::collections::HashMap;
        use std::process::Command;

        let explicit_git_environment = ["GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR"]
            .iter()
            .any(|name| std::env::var_os(name).is_some());
        if !Self::git_marker_present(root)? && !explicit_git_environment {
            return Ok((Vec::new(), VcsContextV1::default()));
        }

        let git_version_output = Command::new("git")
            .arg("--version")
            .current_dir(root)
            .output()
            .map_err(|error| Self::git_failure("git --version", root, error))?;
        if !git_version_output.status.success() {
            return Err(Self::git_status_failure(
                "git --version",
                root,
                &git_version_output,
            ));
        }
        let git_version = Self::single_utf8_line("git --version", &git_version_output.stdout)?;

        let repository_root_output = Self::run_git(root, &["rev-parse", "--show-toplevel"])?;
        let repository_root_raw = Self::single_utf8_line(
            "git rev-parse --show-toplevel",
            &repository_root_output.stdout,
        )?;
        let repository_root = PathBuf::from(&repository_root_raw)
            .canonicalize()
            .map_err(|error| Self::git_failure("canonicalize repository root", root, error))?;
        let repository_root = repository_root
            .to_str()
            .ok_or_else(|| M1ndError::IngestError("Git repository root is not valid UTF-8".into()))?
            .to_string();

        let git_dir_output = Self::run_git(root, &["rev-parse", "--absolute-git-dir"])?;
        let git_dir_raw =
            Self::single_utf8_line("git rev-parse --absolute-git-dir", &git_dir_output.stdout)?;
        let git_dir = PathBuf::from(&git_dir_raw)
            .canonicalize()
            .map_err(|error| Self::git_failure("canonicalize Git directory", root, error))?;
        let git_dir = git_dir
            .to_str()
            .ok_or_else(|| M1ndError::IngestError("Git directory is not valid UTF-8".into()))?
            .to_string();

        let head_output = Command::new("git")
            .args(["rev-parse", "--verify", "HEAD^{commit}"])
            .current_dir(root)
            .output()
            .map_err(|error| Self::git_failure("git rev-parse HEAD", root, error))?;
        let head_oid = if head_output.status.success() {
            let oid = Self::single_utf8_line("git rev-parse HEAD", &head_output.stdout)?;
            if !(oid.len() == 40 || oid.len() == 64)
                || !oid.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(M1ndError::IngestError(format!(
                    "Git HEAD has invalid object identity {oid:?}"
                )));
            }
            Some(oid)
        } else {
            let count_output = Self::run_git(root, &["rev-list", "--all", "--count"])?;
            let count = Self::single_utf8_line("git rev-list --all --count", &count_output.stdout)?
                .parse::<u64>()
                .map_err(|error| {
                    M1ndError::IngestError(format!("Git revision count parse failed: {error}"))
                })?;
            if count == 0 {
                None
            } else {
                return Err(Self::git_status_failure(
                    "git rev-parse --verify HEAD^{commit}",
                    root,
                    &head_output,
                ));
            }
        };

        // Run git log once to get all commits with timestamps and affected files.
        let history_output = if head_oid.is_some() {
            Self::run_git(
                root,
                &[
                    "-c",
                    "core.quotePath=false",
                    "log",
                    "--no-renames",
                    "--relative",
                    "-z",
                    "--format=%x00%at%x00",
                    "--name-only",
                    "--diff-filter=ACDMR",
                ],
            )?
        } else {
            std::process::Output {
                status: Self::success_exit_status(),
                stdout: Vec::new(),
                stderr: Vec::new(),
            }
        };
        let history_output_digest = crate::ownership::sha256_bytes(&history_output.stdout);
        // Parse NUL-framed commits. Empty NUL tokens are structural markers and
        // cannot collide with a Git filename (NUL is forbidden in paths). This
        // avoids treating a valid all-digit filename as a timestamp.
        let mut file_stats: HashMap<String, (u32, f64)> = HashMap::new(); // path -> (count, last_time)
        let mut commit_groups: Vec<Vec<String>> = Vec::new();
        let tokens = history_output
            .stdout
            .split(|byte| *byte == 0)
            .collect::<Vec<_>>();
        let mut cursor = 0usize;
        while cursor < tokens.len() {
            if !tokens[cursor].is_empty() {
                return Err(M1ndError::IngestError(format!(
                    "Git history framing failed at token {cursor}: expected commit boundary"
                )));
            }
            cursor += 1;
            if cursor == tokens.len() {
                break;
            }

            let timestamp_text = std::str::from_utf8(tokens[cursor]).map_err(|error| {
                M1ndError::IngestError(format!(
                    "Git history timestamp is not valid UTF-8 at token {cursor}: {error}"
                ))
            })?;
            if timestamp_text.is_empty()
                || !timestamp_text.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err(M1ndError::IngestError(format!(
                    "Git history timestamp token is invalid: {timestamp_text:?}"
                )));
            }
            let current_timestamp = timestamp_text.parse::<u64>().map_err(|error| {
                M1ndError::IngestError(format!(
                    "Git history timestamp parse failed for {timestamp_text:?}: {error}"
                ))
            })? as f64;
            cursor += 1;
            if cursor >= tokens.len() || !tokens[cursor].is_empty() {
                return Err(M1ndError::IngestError(
                    "Git history framing omitted timestamp terminator".into(),
                ));
            }
            cursor += 1;

            let mut current_group = Vec::new();
            let mut first_path = true;
            while cursor < tokens.len() && !tokens[cursor].is_empty() {
                let raw_path = if first_path {
                    tokens[cursor].strip_prefix(b"\n").ok_or_else(|| {
                        M1ndError::IngestError(
                            "Git history framing omitted first-path separator".into(),
                        )
                    })?
                } else {
                    tokens[cursor]
                };
                let path = std::str::from_utf8(raw_path).map_err(|error| {
                    M1ndError::IngestError(format!(
                        "Git history path is not valid UTF-8 at token {cursor}: {error}"
                    ))
                })?;
                if !crate::is_valid_relative_file_path(path) {
                    return Err(M1ndError::IngestError(format!(
                        "Git history emitted non-bijective path identity {path:?}"
                    )));
                }
                if current_group.iter().any(|existing| existing == path) {
                    return Err(M1ndError::IngestError(format!(
                        "Git history repeated path {path:?} within one commit"
                    )));
                }
                let entry = file_stats.entry(path.to_string()).or_insert((0, 0.0));
                entry.0 += 1;
                if current_timestamp > entry.1 {
                    entry.1 = current_timestamp;
                }
                current_group.push(path.to_string());
                first_path = false;
                cursor += 1;
            }
            if current_group.len() >= 2 {
                commit_groups.push(current_group);
            }
        }

        // Apply to discovered files
        for file in files.iter_mut() {
            if let Some(&(count, last_time)) = file_stats.get(&file.relative_path) {
                file.commit_count = count;
                file.last_commit_time = last_time;
                // Override last_modified with git time if available
                if last_time > 0.0 {
                    file.last_modified = last_time;
                }
            }
        }

        let vcs = VcsContextV1 {
            schema: VCS_CONTEXT_SCHEMA_V1.into(),
            kind: "git".into(),
            git_version: Some(git_version),
            repository_root: Some(repository_root),
            git_dir: Some(git_dir),
            head_oid,
            history_output_digest: Some(history_output_digest),
        };
        if !vcs.valid() {
            return Err(M1ndError::IngestError(
                "Git context failed its exact identity invariant".into(),
            ));
        }
        Ok((commit_groups, vcs))
    }

    fn git_marker_present(root: &Path) -> M1ndResult<bool> {
        for ancestor in root.ancestors() {
            match std::fs::symlink_metadata(ancestor.join(".git")) {
                Ok(_) => return Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(M1ndError::IngestError(format!(
                        "Git marker discovery failed under {}: {error}",
                        ancestor.display()
                    )));
                }
            }
        }
        Ok(false)
    }

    fn run_git(root: &Path, args: &[&str]) -> M1ndResult<std::process::Output> {
        let label = format!("git {}", args.join(" "));
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .map_err(|error| Self::git_failure(&label, root, error))?;
        if !output.status.success() {
            return Err(Self::git_status_failure(&label, root, &output));
        }
        Ok(output)
    }

    fn single_utf8_line(label: &str, bytes: &[u8]) -> M1ndResult<String> {
        let value = String::from_utf8(bytes.to_vec()).map_err(|error| {
            M1ndError::IngestError(format!("{label} output is not valid UTF-8: {error}"))
        })?;
        let value = value.strip_suffix('\n').unwrap_or(&value);
        let value = value.strip_suffix('\r').unwrap_or(value);
        if value.is_empty() || value.contains(['\n', '\r']) || value != value.trim() {
            return Err(M1ndError::IngestError(format!(
                "{label} output is not one exact non-empty line: {value:?}"
            )));
        }
        Ok(value.to_string())
    }

    fn git_failure(label: &str, root: &Path, error: impl std::fmt::Display) -> M1ndError {
        M1ndError::IngestError(format!(
            "{label} failed under {} and cannot be represented as neutral VCS context: {error}",
            root.display()
        ))
    }

    fn git_status_failure(label: &str, root: &Path, output: &std::process::Output) -> M1ndError {
        Self::git_failure(
            label,
            root,
            format!(
                "status {:?}, stderr {:?}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ),
        )
    }

    #[cfg(unix)]
    fn success_exit_status() -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }

    #[cfg(windows)]
    fn success_exit_status() -> std::process::ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }

    /// Check if a file is binary (NUL byte in first 8KB).
    /// FM-ING-004 fix.
    pub fn is_binary(path: &Path) -> M1ndResult<bool> {
        let mut file = std::fs::File::open(path).map_err(M1ndError::Io)?;
        Self::is_binary_reader(&mut file)
    }

    fn is_binary_reader(reader: &mut impl std::io::Read) -> M1ndResult<bool> {
        let mut buf = [0u8; 8192];
        let n = reader.read(&mut buf).map_err(M1ndError::Io)?;
        Ok(buf[..n].contains(&0))
    }
}

#[cfg(test)]
mod tests {
    use super::{DirectoryWalker, DiscoveryProbe, OsDiscoveryProbe};
    use m1nd_core::error::{M1ndError, M1ndResult};
    use std::path::{Path, PathBuf};

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum InjectedFailure {
        BinaryProbe,
        Metadata,
    }

    struct InjectedProbe {
        fail_path: PathBuf,
        failure: InjectedFailure,
    }

    impl InjectedProbe {
        fn new(fail_path: PathBuf, failure: InjectedFailure) -> Self {
            Self { fail_path, failure }
        }

        fn matches(&self, path: &Path, failure: InjectedFailure) -> bool {
            path == self.fail_path && self.failure == failure
        }

        fn injected_io() -> std::io::Error {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected walker I/O failure",
            )
        }
    }

    impl DiscoveryProbe for InjectedProbe {
        fn is_binary(&self, path: &Path) -> M1ndResult<bool> {
            if self.matches(path, InjectedFailure::BinaryProbe) {
                return Err(M1ndError::Io(Self::injected_io()));
            }
            OsDiscoveryProbe.is_binary(path)
        }

        fn path_metadata(&self, path: &Path) -> std::io::Result<std::fs::Metadata> {
            if self.matches(path, InjectedFailure::Metadata) {
                return Err(Self::injected_io());
            }
            OsDiscoveryProbe.path_metadata(path)
        }

        fn entry_metadata(
            &self,
            entry: &ignore::DirEntry,
        ) -> Result<std::fs::Metadata, ignore::Error> {
            if self.matches(entry.path(), InjectedFailure::Metadata) {
                return Err(ignore::Error::WithPath {
                    path: entry.path().to_path_buf(),
                    err: Box::new(ignore::Error::Io(Self::injected_io())),
                });
            }
            OsDiscoveryProbe.entry_metadata(entry)
        }
    }

    struct TempTree(PathBuf);

    impl TempTree {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "m1nd-walker-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            ));
            std::fs::create_dir_all(&path).expect("tempdir");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct FailingReader;

    impl std::io::Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected binary read failure",
            ))
        }
    }

    fn assert_io_error<T: std::fmt::Debug>(result: M1ndResult<T>, expected: &str) {
        match result {
            Err(M1ndError::Io(error)) => assert!(
                error.to_string().contains(expected),
                "expected {expected:?} in {error}"
            ),
            other => panic!("expected I/O error containing {expected:?}, got {other:?}"),
        }
    }

    #[test]
    fn unreadable_scan_root_is_fatal() {
        // The 2026-08-02 inversion: a deeper unreadable entry is now skipped
        // (it can contain nothing ingestable — pinned end to end by
        // `an_unreadable_subdirectory_does_not_abort_the_ingest` in lib.rs),
        // but the SCAN ROOT itself stays fatal: an empty walk over a root we
        // cannot open would silently claim an empty repo.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let root = std::env::temp_dir().join(format!(
                "m1nd-walker-unreadable-root-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o000)).unwrap();
            let result =
                DirectoryWalker::new(Vec::new(), Vec::new(), false, Vec::new()).walk(&root);
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::remove_dir_all(&root).ok();
            let error = result.expect_err("an unreadable scan root must be a hard error");
            assert!(
                error.to_string().contains("cannot open the scan root"),
                "the error must name the root refusal, got: {error}"
            );
        }
    }

    #[test]
    fn binary_reader_error_is_fatal() {
        assert_io_error(
            DirectoryWalker::is_binary_reader(&mut FailingReader),
            "injected binary read failure",
        );
    }

    #[test]
    fn directory_walk_binary_probe_error_is_fatal() {
        let temp = TempTree::new("directory-binary-error");
        let file = temp.path().join("source.rs");
        std::fs::write(&file, "pub fn source() {}\n").expect("write source");
        let canonical_file = file.canonicalize().expect("canonical source");
        let probe = InjectedProbe::new(canonical_file, InjectedFailure::BinaryProbe);
        let walker = DirectoryWalker::new(Vec::new(), Vec::new(), false, Vec::new());

        assert_io_error(
            walker.walk_with_probe(temp.path(), &probe),
            "injected walker I/O failure",
        );
    }

    #[test]
    fn directory_walk_metadata_error_is_fatal() {
        let temp = TempTree::new("directory-metadata-error");
        let file = temp.path().join("source.rs");
        std::fs::write(&file, "pub fn source() {}\n").expect("write source");
        let canonical_file = file.canonicalize().expect("canonical source");
        let probe = InjectedProbe::new(canonical_file, InjectedFailure::Metadata);
        let walker = DirectoryWalker::new(Vec::new(), Vec::new(), false, Vec::new());

        assert_io_error(
            walker.walk_with_probe(temp.path(), &probe),
            "injected walker I/O failure",
        );
    }

    #[test]
    fn single_file_binary_probe_error_is_fatal() {
        let temp = TempTree::new("single-binary-error");
        let file = temp.path().join("source.rs");
        std::fs::write(&file, "pub fn source() {}\n").expect("write source");
        let canonical_file = file.canonicalize().expect("canonical source");
        let probe = InjectedProbe::new(canonical_file, InjectedFailure::BinaryProbe);
        let walker = DirectoryWalker::new(Vec::new(), Vec::new(), false, Vec::new());

        assert_io_error(
            walker.walk_with_probe(&file, &probe),
            "injected walker I/O failure",
        );
    }

    #[test]
    fn single_file_metadata_error_is_fatal() {
        let temp = TempTree::new("single-metadata-error");
        let file = temp.path().join("source.rs");
        std::fs::write(&file, "pub fn source() {}\n").expect("write source");
        let canonical_file = file.canonicalize().expect("canonical source");
        let probe = InjectedProbe::new(canonical_file, InjectedFailure::Metadata);
        let walker = DirectoryWalker::new(Vec::new(), Vec::new(), false, Vec::new());

        assert_io_error(
            walker.walk_with_probe(&file, &probe),
            "injected walker I/O failure",
        );
    }

    #[test]
    fn walk_single_file_uses_filename_as_relative_path() {
        let temp = std::env::temp_dir().join(format!(
            "m1nd-walker-single-file-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp).expect("tempdir");
        let file = temp.join("core.rs");
        std::fs::write(&file, "pub fn core() {}\n").expect("write file");

        let walker = DirectoryWalker::new(Vec::new(), Vec::new(), false, Vec::new());
        let result = walker.walk(&file).expect("walk single file");

        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].relative_path, "core.rs");
        assert_eq!(
            result.files[0].path,
            file.canonicalize().expect("canonical file")
        );
        std::fs::remove_dir_all(temp).expect("cleanup tempdir");
    }

    #[test]
    fn non_git_root_is_explicitly_unversioned() {
        let temp = TempTree::new("explicit-unversioned");
        std::fs::write(temp.path().join("source.rs"), "pub fn source() {}\n")
            .expect("write source");
        let walker = DirectoryWalker::new(Vec::new(), Vec::new(), false, Vec::new());

        let result = walker.walk(temp.path()).expect("walk non-Git root");
        assert_eq!(result.vcs.kind, "unversioned");
        assert!(result.vcs.valid());
        assert!(result.vcs.head_oid.is_none());
    }

    #[test]
    fn broken_git_discovery_is_fatal_not_neutral() {
        let temp = TempTree::new("broken-git");
        std::fs::write(temp.path().join("source.rs"), "pub fn source() {}\n")
            .expect("write source");
        std::fs::write(
            temp.path().join(".git"),
            "gitdir: /definitely/missing/m1nd-git-dir\n",
        )
        .expect("write broken Git marker");
        let walker = DirectoryWalker::new(Vec::new(), Vec::new(), false, Vec::new());

        let error = walker
            .walk(temp.path())
            .expect_err("broken Git marker must not become unversioned");
        assert!(
            error
                .to_string()
                .contains("cannot be represented as neutral VCS context"),
            "{error}"
        );
    }

    #[test]
    fn unborn_git_repository_is_git_not_neutral() {
        let temp = TempTree::new("unborn-git-context");
        std::fs::write(temp.path().join("source.rs"), "pub fn source() {}\n")
            .expect("write source");
        let output = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(temp.path())
            .output()
            .expect("init Git fixture");
        assert!(output.status.success());

        let walker = DirectoryWalker::new(Vec::new(), Vec::new(), false, Vec::new());
        let result = walker.walk(temp.path()).expect("walk unborn Git root");
        assert_eq!(result.vcs.kind, "git");
        assert!(result.vcs.head_oid.is_none());
        assert_eq!(
            result.vcs.history_output_digest,
            Some(crate::ownership::sha256_bytes(&[]))
        );
        assert!(result.vcs.valid());
    }

    #[test]
    fn git_context_binds_exact_repository_head_and_history_producer() {
        let temp = TempTree::new("exact-git-context");
        std::fs::write(temp.path().join("source.rs"), "pub fn source() {}\n")
            .expect("write source");
        std::fs::write(temp.path().join("12345678"), "numeric filename\n")
            .expect("write numeric filename");
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "m1nd-test@example.invalid"],
            vec!["config", "user.name", "m1nd test"],
            vec!["add", "source.rs", "12345678"],
            vec!["-c", "commit.gpgsign=false", "commit", "-qm", "fixture"],
        ] {
            let output = std::process::Command::new("git")
                .args(&args)
                .current_dir(temp.path())
                .output()
                .expect("run Git fixture command");
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let walker = DirectoryWalker::new(Vec::new(), Vec::new(), false, Vec::new());
        let result = walker.walk(temp.path()).expect("walk Git root");
        assert_eq!(result.vcs.kind, "git");
        assert!(result.vcs.valid());
        assert_eq!(result.vcs.head_oid.as_deref().map(str::len), Some(40));
        assert_eq!(
            result.vcs.history_output_digest.as_deref().map(str::len),
            Some(64)
        );
        assert!(result
            .vcs
            .git_version
            .as_deref()
            .unwrap()
            .starts_with("git version "));
        assert_eq!(result.files.len(), 2);
        assert!(result.files.iter().all(|file| file.commit_count == 1));
        assert!(result
            .files
            .iter()
            .any(|file| file.relative_path == "12345678"));
    }

    #[test]
    fn walk_respects_gitignore() {
        let temp = std::env::temp_dir().join(format!(
            "m1nd-walker-gitignore-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(temp.join("vendored")).expect("vendored dir");
        std::fs::create_dir_all(temp.join("src")).expect("src dir");
        std::fs::write(temp.join(".gitignore"), "vendored/\n").expect("gitignore");
        std::fs::write(temp.join("vendored").join("junk.rs"), "pub fn junk() {}\n")
            .expect("junk file");
        std::fs::write(temp.join("src").join("real.rs"), "pub fn real() {}\n").expect("real file");

        // `git init` makes the .gitignore deterministically honored by the ignore crate.
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&temp)
            .status()
            .expect("git init");
        assert!(status.success(), "git init failed");

        let walker = DirectoryWalker::new(Vec::new(), Vec::new(), false, Vec::new());
        let result = walker.walk(&temp).expect("walk gitignore tree");

        let rels: Vec<&str> = result
            .files
            .iter()
            .map(|f| f.relative_path.as_str())
            .collect();

        assert!(
            rels.contains(&"src/real.rs"),
            "expected src/real.rs to be discovered, got {rels:?}"
        );
        assert!(
            !rels.contains(&"vendored/junk.rs"),
            "expected vendored/junk.rs to be gitignored, got {rels:?}"
        );

        std::fs::remove_dir_all(temp).expect("cleanup tempdir");
    }
}
