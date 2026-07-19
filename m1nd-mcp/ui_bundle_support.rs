use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const UI_TREE_DIGEST_DOMAIN: &[u8] = b"m1nd-ui-bundle-tree-v1\0";
pub const UI_PLACEHOLDER_MARKER: &[u8] = b"m1nd UI not built";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiTreeIdentity {
    pub sha256: String,
    pub placeholder: bool,
}

#[derive(Debug)]
pub enum StableUiTreeError {
    Io(std::io::Error),
    Unstable {
        before: UiTreeIdentity,
        after: Option<UiTreeIdentity>,
        detail: String,
    },
}

impl std::fmt::Display for StableUiTreeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "unable to read UI tree: {error}"),
            Self::Unstable {
                before,
                after,
                detail,
            } => write!(
                formatter,
                "UI tree changed while it was attested (before={}, after={}, detail={detail})",
                before.sha256,
                after
                    .as_ref()
                    .map(|identity| identity.sha256.as_str())
                    .unwrap_or("unreadable")
            ),
        }
    }
}

impl std::error::Error for StableUiTreeError {}

/// Content identity of a served UI tree. Enumeration order and metadata do not
/// affect the digest; every `(relative path, bytes)` pair is length-delimited.
pub fn ui_tree_identity(root: &Path) -> std::io::Result<UiTreeIdentity> {
    fn collect(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(current)? {
            let path = entry?.path();
            if path.is_dir() {
                collect(root, &path, files)?;
            } else if path.is_file() {
                files.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    collect(root, root, &mut files)?;
    files.sort();

    let mut entries = Vec::with_capacity(files.len());
    for relative in files {
        let normalized = relative.to_string_lossy().replace('\\', "/");
        let bytes = std::fs::read(root.join(&relative))?;
        entries.push((normalized, bytes));
    }
    Ok(ui_tree_identity_from_entries(entries))
}

/// The same framing over an already-materialized tree. Release HTTP serving
/// uses this over `UiAssets::iter/get`, proving that the build record names the
/// bytes that actually made it into the binary rather than merely the source
/// directory as it looked before rust-embed ran.
pub fn ui_tree_identity_from_entries(mut entries: Vec<(String, Vec<u8>)>) -> UiTreeIdentity {
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    hasher.update(UI_TREE_DIGEST_DOMAIN);
    let mut placeholder = false;
    for (normalized, bytes) in entries {
        if normalized == "index.html"
            && bytes
                .windows(UI_PLACEHOLDER_MARKER.len())
                .any(|window| window == UI_PLACEHOLDER_MARKER)
        {
            placeholder = true;
        }
        hasher.update((normalized.len() as u64).to_be_bytes());
        hasher.update(normalized.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    UiTreeIdentity {
        sha256: format!("{:x}", hasher.finalize()),
        placeholder,
    }
}

/// Double-observe the tree. A caller never receives a digest assembled across a
/// changing filesystem: a mismatch or second-pass read failure is instability,
/// not a fresh authority fact.
pub fn stable_ui_tree_identity(root: &Path) -> Result<UiTreeIdentity, StableUiTreeError> {
    stable_ui_tree_identity_with_hook(root, || {})
}

pub fn stable_ui_tree_identity_with_hook(
    root: &Path,
    between_observations: impl FnOnce(),
) -> Result<UiTreeIdentity, StableUiTreeError> {
    let before = ui_tree_identity(root).map_err(StableUiTreeError::Io)?;
    between_observations();
    let after = match ui_tree_identity(root) {
        Ok(identity) => identity,
        Err(error) => {
            return Err(StableUiTreeError::Unstable {
                before,
                after: None,
                detail: error.to_string(),
            })
        }
    };
    if before != after {
        return Err(StableUiTreeError::Unstable {
            before,
            after: Some(after),
            detail: "two consecutive identities differ".to_string(),
        });
    }
    Ok(after)
}
