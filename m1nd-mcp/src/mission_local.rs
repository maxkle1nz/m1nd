//! HUMAN VIEW v2 — F2.5a: the owner-runtime-local mission side record (§1f).
//!
//! The public mission letter ([`crate::mission_letter::MissionLetter`]) carries
//! NO absolute paths — `brain_ref` is a reference string, and the `worktree`
//! field was removed from the schema (oracle objection 7). The host-local detail
//! a mission still needs — the worktree path an agent ran in, free-form host
//! notes — lives HERE instead: a sidecar keyed by `mission_id`, stored next to
//! the brain's other runtime artifacts (the runtime root), **never versionable
//! and never served raw off-loopback**. The tray fetches it only for local
//! display (§3f); this module is the load/save library for it, no route.
//!
//! Keeping this type in its OWN module is the structural half of §1f: the public
//! contract module physically cannot serialize an absolute path because the
//! path-bearing type is not in it. The absolute worktree path is legal HERE — a
//! host-local record — and only here.
//!
//! Persistence mirrors the SystemBlock store's atomic pattern
//! ([`crate::system_blocks::SystemBlockStore::save`]): write a sibling temp file,
//! then rename over the target, so a reader never sees a half-written record.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use m1nd_core::error::{M1ndError, M1ndResult};

/// The sidecar file name inside the owner runtime root.
pub const MISSION_LOCAL_FILE: &str = "mission_local.json";

/// The frozen schema tag for the side record.
pub const MISSION_LOCAL_SCHEMA: &str = "m1nd-mission-local-v0";

/// One mission's host-local detail (§1f). The `worktree` path is an ABSOLUTE host
/// path — legal here (this record is never public, never git-traveled), never in
/// the public letter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionLocalRecord {
    /// The absolute worktree path the mission ran in (owner-runtime-local only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    /// Free-form host notes (which local runner, machine, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_notes: Option<String>,
}

/// The owner-runtime-local mission side record — a map `mission_id → record`,
/// stored atomically. Owner-runtime-local: it lives under the runtime root and is
/// never served raw off-loopback (§1f).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionLocalStore {
    pub schema: String,
    #[serde(default)]
    pub records: BTreeMap<String, MissionLocalRecord>,
}

impl Default for MissionLocalStore {
    fn default() -> Self {
        Self {
            schema: MISSION_LOCAL_SCHEMA.to_string(),
            records: BTreeMap::new(),
        }
    }
}

impl MissionLocalStore {
    /// The side-record path inside an owner runtime root.
    pub fn path_in(runtime_root: &Path) -> PathBuf {
        runtime_root.join(MISSION_LOCAL_FILE)
    }

    /// Load the side record from the runtime root. A missing file is an honest
    /// empty store (never an error) — a runtime that never posted a mission simply
    /// has none.
    pub fn load(runtime_root: &Path) -> M1ndResult<Self> {
        let path = Self::path_in(runtime_root);
        let raw = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(M1ndError::Io(e)),
        };
        let store: MissionLocalStore = serde_json::from_str(&raw).map_err(M1ndError::Serde)?;
        Ok(store)
    }

    /// Persist atomically: write a sibling temp file, then rename over the target
    /// (mirrors [`crate::system_blocks::SystemBlockStore::save`]).
    pub fn save(&self, runtime_root: &Path) -> M1ndResult<()> {
        std::fs::create_dir_all(runtime_root).map_err(M1ndError::Io)?;
        let path = Self::path_in(runtime_root);
        let tmp = path.with_extension("json.tmp");
        let payload = serde_json::to_vec_pretty(self).map_err(M1ndError::Serde)?;
        std::fs::write(&tmp, payload).map_err(M1ndError::Io)?;
        std::fs::rename(&tmp, &path).map_err(M1ndError::Io)?;
        Ok(())
    }

    /// Upsert one mission's host-local record and persist atomically.
    pub fn put(
        runtime_root: &Path,
        mission_id: &str,
        record: MissionLocalRecord,
    ) -> M1ndResult<()> {
        let mut store = Self::load(runtime_root)?;
        store.records.insert(mission_id.to_string(), record);
        store.save(runtime_root)
    }

    /// Read one mission's host-local record (`None` when absent).
    pub fn get(runtime_root: &Path, mission_id: &str) -> M1ndResult<Option<MissionLocalRecord>> {
        Ok(Self::load(runtime_root)?.records.get(mission_id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch {
        dir: PathBuf,
    }
    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "m1nd-mission-local-test-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&dir).expect("mk scratch");
            Self { dir }
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn missing_file_loads_as_empty_store() {
        let s = Scratch::new("empty");
        let store = MissionLocalStore::load(&s.dir).unwrap();
        assert_eq!(store.schema, MISSION_LOCAL_SCHEMA);
        assert!(store.records.is_empty());
    }

    #[test]
    fn save_load_roundtrip_is_atomic_and_absolute_worktree_lives_here() {
        let s = Scratch::new("roundtrip");
        // The absolute worktree path is LEGAL here (host-local record) — this is
        // exactly the detail §1f keeps OUT of the public letter and IN this sidecar.
        let rec = MissionLocalRecord {
            worktree: Some("/path/to/worktrees/mission-x".to_string()),
            host_notes: Some("build-runner on host alpha".to_string()),
        };
        MissionLocalStore::put(&s.dir, "msn_0123456789ab", rec.clone()).unwrap();

        // No stray temp file remains after the atomic rename.
        assert!(!MissionLocalStore::path_in(&s.dir)
            .with_extension("json.tmp")
            .exists());

        let loaded = MissionLocalStore::get(&s.dir, "msn_0123456789ab")
            .unwrap()
            .expect("record present");
        assert_eq!(loaded, rec);
        assert_eq!(
            loaded.worktree.as_deref(),
            Some("/path/to/worktrees/mission-x"),
            "the absolute worktree path survives — it belongs to the host-local record"
        );

        // A second mission upserts without clobbering the first.
        MissionLocalStore::put(
            &s.dir,
            "msn_ffffffffffff",
            MissionLocalRecord {
                worktree: None,
                host_notes: Some("naming-runner".to_string()),
            },
        )
        .unwrap();
        let store = MissionLocalStore::load(&s.dir).unwrap();
        assert_eq!(store.records.len(), 2, "both missions kept");
        assert!(store.records.contains_key("msn_0123456789ab"));
    }
}
