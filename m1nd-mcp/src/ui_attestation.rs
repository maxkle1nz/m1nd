use std::path::{Path, PathBuf};

use m1nd_control::{AuthorityFreshness, AuthorityStatus};

use crate::ui_bundle_support::{stable_ui_tree_identity, StableUiTreeError, UiTreeIdentity};

pub const UI_MODE_EMBEDDED: &str = "embedded";
pub const UI_MODE_DEBUG_FILESYSTEM: &str = "debug_filesystem";
pub const UI_MODE_DEVELOPMENT_DIST: &str = "development_dist";
pub const UI_MODE_EXTERNAL_DIR: &str = "external_ui_dir";

#[derive(Clone, Debug)]
enum UiServingSource {
    EmbeddedRelease,
    RustEmbedDebug { root: PathBuf },
    ServeDir { root: PathBuf, mode: &'static str },
}

/// One configuration shared by the HTTP fallback and `/api/manifest`. This is
/// the binding that prevents the manifest from hashing one tree while Axum
/// serves another.
#[derive(Clone, Debug)]
pub struct UiBundleAttestor {
    source: UiServingSource,
    build_version: String,
    build_sha256: String,
    build_placeholder: bool,
}

#[derive(Clone, Debug)]
pub struct UiBundleObservation {
    pub bundle_version: String,
    pub bundle_sha256: String,
    pub mode: String,
    pub status: AuthorityStatus,
    pub freshness: AuthorityFreshness,
}

impl Default for UiBundleAttestor {
    fn default() -> Self {
        Self::for_http(false, None)
    }
}

impl UiBundleAttestor {
    /// Resolve the actual serving source once at owner boot.
    ///
    /// - explicit `--ui-dir` always selects that filesystem tree;
    /// - `--dev` selects the workspace `m1nd-ui/dist` through `ServeDir`;
    /// - a debug rust-embed build reads that same tree dynamically at runtime;
    /// - only a non-debug default build uses build-time embedded bytes.
    pub fn for_http(dev_mode: bool, ui_dir: Option<PathBuf>) -> Self {
        let source = if let Some(root) = ui_dir {
            UiServingSource::ServeDir {
                root,
                mode: UI_MODE_EXTERNAL_DIR,
            }
        } else if dev_mode {
            UiServingSource::ServeDir {
                root: default_ui_dist(),
                mode: UI_MODE_DEVELOPMENT_DIST,
            }
        } else if cfg!(debug_assertions) {
            UiServingSource::RustEmbedDebug {
                root: default_ui_dist(),
            }
        } else {
            UiServingSource::EmbeddedRelease
        };
        Self {
            source,
            build_version: env!("M1ND_UI_BUNDLE_VERSION").to_string(),
            build_sha256: known_build_digest(env!("M1ND_UI_BUNDLE_SHA256")),
            build_placeholder: env!("M1ND_UI_BUNDLE_PLACEHOLDER") == "1",
        }
    }

    pub fn serve_dir(&self) -> Option<PathBuf> {
        match &self.source {
            UiServingSource::ServeDir { root, .. } => Some(root.clone()),
            UiServingSource::EmbeddedRelease | UiServingSource::RustEmbedDebug { .. } => None,
        }
    }

    pub fn mode(&self) -> &'static str {
        match &self.source {
            UiServingSource::EmbeddedRelease => UI_MODE_EMBEDDED,
            UiServingSource::RustEmbedDebug { .. } => UI_MODE_DEBUG_FILESYSTEM,
            UiServingSource::ServeDir { mode, .. } => mode,
        }
    }

    pub fn observes_filesystem(&self) -> bool {
        !matches!(&self.source, UiServingSource::EmbeddedRelease)
    }

    pub fn observe(&self) -> Result<UiBundleObservation, String> {
        match &self.source {
            // Without materializing `UiAssets`, the build record alone cannot
            // prove what rust-embed actually compiled. HTTP calls the explicit
            // `observe_embedded_identity` seam below; other callers degrade.
            UiServingSource::EmbeddedRelease => Ok(self.unverified_embedded_observation()),
            UiServingSource::RustEmbedDebug { root } => self.filesystem_observation(root),
            UiServingSource::ServeDir { root, .. } => self.filesystem_observation(root),
        }
    }

    pub fn observe_embedded_identity(
        &self,
        identity: UiTreeIdentity,
    ) -> Result<UiBundleObservation, String> {
        if !matches!(&self.source, UiServingSource::EmbeddedRelease) {
            return Err("embedded UI identity supplied for a filesystem serving mode".to_string());
        }
        Ok(self.classify_runtime_identity(identity, nonempty_revision(&self.build_version)))
    }

    fn unverified_embedded_observation(&self) -> UiBundleObservation {
        if self.build_sha256.is_empty() {
            return unavailable(self.mode());
        }
        UiBundleObservation {
            bundle_version: nonempty_revision(&self.build_version),
            bundle_sha256: self.build_sha256.clone(),
            mode: self.mode().to_string(),
            status: AuthorityStatus::Degraded,
            freshness: AuthorityFreshness::Unknown,
        }
    }

    fn filesystem_observation(&self, root: &Path) -> Result<UiBundleObservation, String> {
        let identity = match stable_ui_tree_identity(root) {
            Ok(identity) => identity,
            Err(StableUiTreeError::Io(_)) => return Ok(unavailable(self.mode())),
            Err(error @ StableUiTreeError::Unstable { .. }) => return Err(error.to_string()),
        };
        let bundle_version = if self.mode() == UI_MODE_EXTERNAL_DIR {
            runtime_package_version(root).unwrap_or_else(|| "runtime-tree".to_string())
        } else {
            nonempty_revision(&self.build_version)
        };
        Ok(self.classify_runtime_identity(identity, bundle_version))
    }

    fn classify_runtime_identity(
        &self,
        identity: UiTreeIdentity,
        bundle_version: String,
    ) -> UiBundleObservation {
        let runtime_sha256 = format!("sha256:{}", identity.sha256);
        let baseline_known = !self.build_sha256.is_empty() && !self.build_placeholder;
        let (status, freshness) = if identity.placeholder || !baseline_known {
            (AuthorityStatus::Degraded, AuthorityFreshness::Unknown)
        } else if runtime_sha256 == self.build_sha256 {
            (AuthorityStatus::Available, AuthorityFreshness::Fresh)
        } else {
            (AuthorityStatus::Drift, AuthorityFreshness::Fresh)
        };
        UiBundleObservation {
            bundle_version,
            bundle_sha256: runtime_sha256,
            mode: self.mode().to_string(),
            status,
            freshness,
        }
    }
}

fn default_ui_dist() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../m1nd-ui/dist")
}

fn runtime_package_version(root: &Path) -> Option<String> {
    let package_json = root.parent()?.join("package.json");
    std::fs::read(package_json)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value.get("version")?.as_str().map(str::to_owned))
        .filter(|version| !version.trim().is_empty())
}

fn known_build_digest(raw: &str) -> String {
    if raw.is_empty() || raw == "unknown" {
        String::new()
    } else {
        format!("sha256:{raw}")
    }
}

fn nonempty_revision(revision: &str) -> String {
    if revision.trim().is_empty() || revision == "unknown" {
        "runtime-tree".to_string()
    } else {
        revision.to_string()
    }
}

fn unavailable(mode: &str) -> UiBundleObservation {
    UiBundleObservation {
        bundle_version: String::new(),
        bundle_sha256: String::new(),
        mode: mode.to_string(),
        status: AuthorityStatus::Unavailable,
        freshness: AuthorityFreshness::Unknown,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_bundle_uses_the_organism_version() {
        assert_eq!(env!("M1ND_UI_BUNDLE_VERSION"), env!("CARGO_PKG_VERSION"));
    }
}
