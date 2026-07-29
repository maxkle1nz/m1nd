//! One-way, conservative retirement of the legacy arbitrary Boot KV store.
//!
//! Typed configuration entries move to `boot_config_v1.json`; every other
//! entry becomes a provenance-bearing L1GHT document. A durable journal holds
//! the exact original bytes and the complete deterministic plan, so restart can
//! replay forward and an explicit rollback can restore the old store byte for
//! byte. The active writer is retired only after target conservation verifies.

use crate::session::{BootMemoryEntry, BootMemoryState};
use m1nd_core::error::{M1ndError, M1ndResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Component, Path};
use std::sync::atomic::{AtomicU64, Ordering};

pub const LEGACY_BOOT_KV_FILE: &str = "boot_memory_state.json";
pub const BOOT_CONFIG_FILE: &str = "boot_config_v1.json";
pub const MIGRATION_MARKER_FILE: &str = "boot_kv_migration_v1.json";
pub const MIGRATION_JOURNAL_FILE: &str = "boot_kv_migration_journal_v1.json";

const CONFIG_SCHEMA: &str = "m1nd-boot-config-v1";
const MIGRATION_SCHEMA: &str = "m1nd-boot-kv-migration-v1";
const JOURNAL_SCHEMA: &str = "m1nd-boot-kv-migration-journal-v1";
const VERSION: u32 = 1;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BootConfigStateV1 {
    pub schema: String,
    pub version: u32,
    pub entries: BTreeMap<String, BootMemoryEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BootKvMigrationMarkerV1 {
    pub schema: String,
    pub version: u32,
    pub status: String,
    pub source_path: String,
    pub source_existed: bool,
    pub source_digest: String,
    pub source_count: usize,
    pub config_count: usize,
    pub light_count: usize,
    pub conservation_digest: String,
    pub config_digest: String,
    pub light_digests: BTreeMap<String, String>,
    pub committed_at_ms: u64,
    pub state_digest: String,
}

/// Fully validated, in-memory working-set projection for candidate-first brain
/// checkpoints.  Fixed migration files carry an explicit `Some(bytes)` /
/// `None` presence decision; migrated L1GHT files are a sorted dynamic set
/// derived from the durable journal rather than re-read while checkpointing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootKvCheckpointInventoryV1 {
    fixed_files: BTreeMap<String, Option<Vec<u8>>>,
    migrated_lights: BTreeMap<String, Vec<u8>>,
}

impl BootKvCheckpointInventoryV1 {
    pub fn fixed_file(&self, relative_path: &str) -> Option<&Option<Vec<u8>>> {
        self.fixed_files.get(relative_path)
    }

    pub fn migrated_lights(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.migrated_lights
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct MigratedCompatibilityEntry {
    pub entry: BootMemoryEntry,
    pub storage: String,
    pub target: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct MigrationJournalV1 {
    schema: String,
    version: u32,
    phase: String,
    source_existed: bool,
    source_raw: String,
    source_digest: String,
    source: BootMemoryState,
    config: BootConfigStateV1,
    semantic_keys: Vec<String>,
    light_files: BTreeMap<String, String>,
    config_preexisting: bool,
    light_preexisting: BTreeSet<String>,
    marker: BootKvMigrationMarkerV1,
    state_digest: String,
}

#[derive(Serialize)]
struct MarkerDigestView<'a> {
    schema: &'a str,
    version: u32,
    status: &'a str,
    source_path: &'a str,
    source_existed: bool,
    source_digest: &'a str,
    source_count: usize,
    config_count: usize,
    light_count: usize,
    conservation_digest: &'a str,
    config_digest: &'a str,
    light_digests: &'a BTreeMap<String, String>,
    committed_at_ms: u64,
}

#[derive(Serialize)]
struct JournalDigestView<'a> {
    schema: &'a str,
    version: u32,
    phase: &'a str,
    source_existed: bool,
    source_raw: &'a str,
    source_digest: &'a str,
    source: BTreeMap<String, BootMemoryEntry>,
    config: &'a BootConfigStateV1,
    semantic_keys: &'a [String],
    light_files: &'a BTreeMap<String, String>,
    config_preexisting: bool,
    light_preexisting: &'a BTreeSet<String>,
    marker: &'a BootKvMigrationMarkerV1,
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    crate::util::hex_lower(&hasher.finalize())
}

fn domain_digest<T: Serialize>(domain: &[u8], value: &T) -> M1ndResult<String> {
    let bytes = serde_json::to_vec(value)?;
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    Ok(crate::util::hex_lower(&hasher.finalize()))
}

fn marker_digest(marker: &BootKvMigrationMarkerV1) -> M1ndResult<String> {
    domain_digest(
        b"m1nd/boot-kv-migration-marker/v1\0",
        &MarkerDigestView {
            schema: &marker.schema,
            version: marker.version,
            status: &marker.status,
            source_path: &marker.source_path,
            source_existed: marker.source_existed,
            source_digest: &marker.source_digest,
            source_count: marker.source_count,
            config_count: marker.config_count,
            light_count: marker.light_count,
            conservation_digest: &marker.conservation_digest,
            config_digest: &marker.config_digest,
            light_digests: &marker.light_digests,
            committed_at_ms: marker.committed_at_ms,
        },
    )
}

fn journal_digest(journal: &MigrationJournalV1) -> M1ndResult<String> {
    domain_digest(
        b"m1nd/boot-kv-migration-journal/v1\0",
        &JournalDigestView {
            schema: &journal.schema,
            version: journal.version,
            phase: &journal.phase,
            source_existed: journal.source_existed,
            source_raw: &journal.source_raw,
            source_digest: &journal.source_digest,
            source: sorted_entries(&journal.source),
            config: &journal.config,
            semantic_keys: &journal.semantic_keys,
            light_files: &journal.light_files,
            config_preexisting: journal.config_preexisting,
            light_preexisting: &journal.light_preexisting,
            marker: &journal.marker,
        },
    )
}

pub(crate) fn durable_atomic_write(path: &Path, bytes: &[u8]) -> M1ndResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| M1ndError::PersistenceFailed("migration target has no filename".into()))?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(".{name}.tmp-{}-{sequence}", std::process::id()));
    let result = (|| -> M1ndResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)?;
        // Windows refuses fsync on directory handles; write-through covers renames.
        #[cfg(not(windows))]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> M1ndResult<()> {
    durable_atomic_write(path, &serde_json::to_vec_pretty(value)?)
}

fn validate_entry_map(state: &BootMemoryState) -> M1ndResult<()> {
    for (map_key, entry) in &state.entries {
        if map_key.trim().is_empty() || entry.key != *map_key {
            return Err(M1ndError::CorruptState {
                reason: format!(
                    "legacy Boot KV key mismatch: map key='{map_key}', entry key='{}'",
                    entry.key
                ),
            });
        }
    }
    Ok(())
}

fn sorted_entries(state: &BootMemoryState) -> BTreeMap<String, BootMemoryEntry> {
    state
        .entries
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn conservation_digest(entries: &BTreeMap<String, BootMemoryEntry>) -> M1ndResult<String> {
    domain_digest(b"m1nd/boot-kv-entry-set/v1\0", entries)
}

fn is_explicit_config(entry: &BootMemoryEntry) -> bool {
    let key = entry.key.to_ascii_lowercase();
    key.starts_with("config.")
        || key.starts_with("boot.config.")
        || entry.tags.iter().any(|tag| {
            let tag = tag.trim().to_ascii_lowercase();
            tag == "config" || tag == "boot-config"
        })
}

fn safe_line(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch == '\n' || ch == '\r' { ' ' } else { ch })
        .collect()
}

fn slug(value: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            separator = false;
        } else if !separator && !output.is_empty() {
            output.push('-');
            separator = true;
        }
        if output.len() >= 48 {
            break;
        }
    }
    output.trim_matches('-').to_string()
}

fn render_light(entry: &BootMemoryEntry) -> M1ndResult<String> {
    let key = safe_line(&entry.key);
    let agent = safe_line(&entry.updated_by_agent);
    let value_json = serde_json::to_string(&entry.value)?;
    let metadata_json = serde_json::to_string(entry)?;
    let mut out = format!(
        "---\nProtocol: L1GHT/1.0\nNode: boot-kv-migration::{key}\nState: migrated\nCreated: {}\nSource-Agent: {agent}\nOrigin-Brain: legacy-boot-kv\nMigration-Source: {LEGACY_BOOT_KV_FILE}\n---\n\n# Migrated Boot KV: {key}\n\n## Conserved semantic entry\n\nLegacy Boot KV entry `{key}` migrated without semantic reinterpretation.\n\n[⍂ entity: {key}]\n[𝔻 confidence: legacy-exact]\n[𝔻 evidence: {LEGACY_BOOT_KV_FILE}]\n\nMigrated-Value-JSON: {value_json}\n\nMigrated-Entry-JSON: {metadata_json}\n",
        entry.updated_at_ms
    );
    for source in &entry.source_refs {
        out.push_str(&format!("\n[𝔻 evidence: {}]", safe_line(source)));
    }
    out.push('\n');
    Ok(out)
}

fn light_relative_path(entry: &BootMemoryEntry) -> M1ndResult<String> {
    let digest = sha256(serde_json::to_string(entry)?.as_bytes());
    let base = slug(&entry.key);
    Ok(format!(
        "agent-memory/boot-kv-{}-{}.light.md",
        if base.is_empty() { "entry" } else { &base },
        &digest[..12]
    ))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn existing_matches(path: &Path, expected: &[u8]) -> M1ndResult<bool> {
    match read_optional_regular_file(path)? {
        Some(actual) if actual == expected => Ok(true),
        Some(_) => Err(M1ndError::PersistenceFailed(format!(
            "migration target collision at '{}': existing bytes differ",
            path.display()
        ))),
        None => Ok(false),
    }
}

fn build_plan(root: &Path) -> M1ndResult<MigrationJournalV1> {
    let source_path = root.join(LEGACY_BOOT_KV_FILE);
    let (source_existed, source_raw, source) = match read_optional_regular_file(&source_path)? {
        Some(bytes) => {
            let source: BootMemoryState = serde_json::from_slice(&bytes)?;
            let raw = String::from_utf8(bytes).map_err(|error| M1ndError::CorruptState {
                reason: format!("legacy Boot KV is not UTF-8 JSON: {error}"),
            })?;
            (true, raw, source)
        }
        None => (false, String::new(), BootMemoryState::default()),
    };
    validate_entry_map(&source)?;
    let all_entries = sorted_entries(&source);
    let source_digest = if source_existed {
        sha256(source_raw.as_bytes())
    } else {
        sha256(b"<absent>")
    };

    let mut config_entries = BTreeMap::new();
    let mut semantic_keys = Vec::new();
    let mut light_files = BTreeMap::new();
    for (key, entry) in &all_entries {
        if is_explicit_config(entry) {
            config_entries.insert(key.clone(), entry.clone());
        } else {
            semantic_keys.push(key.clone());
            light_files.insert(light_relative_path(entry)?, render_light(entry)?);
        }
    }
    semantic_keys.sort();
    let config = BootConfigStateV1 {
        schema: CONFIG_SCHEMA.into(),
        version: VERSION,
        entries: config_entries,
    };
    let config_bytes = serde_json::to_vec_pretty(&config)?;
    let mut light_digests = BTreeMap::new();
    for (relative, contents) in &light_files {
        light_digests.insert(relative.clone(), sha256(contents.as_bytes()));
    }

    let config_path = root.join(BOOT_CONFIG_FILE);
    let config_preexisting = existing_matches(&config_path, &config_bytes)?;
    let mut light_preexisting = BTreeSet::new();
    for (relative, contents) in &light_files {
        if existing_matches(&root.join(relative), contents.as_bytes())? {
            light_preexisting.insert(relative.clone());
        }
    }
    let mut marker = BootKvMigrationMarkerV1 {
        schema: MIGRATION_SCHEMA.into(),
        version: VERSION,
        status: "committed".into(),
        source_path: LEGACY_BOOT_KV_FILE.into(),
        source_existed,
        source_digest: source_digest.clone(),
        source_count: all_entries.len(),
        config_count: config.entries.len(),
        light_count: semantic_keys.len(),
        conservation_digest: conservation_digest(&all_entries)?,
        config_digest: sha256(&config_bytes),
        light_digests,
        committed_at_ms: now_ms(),
        state_digest: String::new(),
    };
    marker.state_digest = marker_digest(&marker)?;
    let mut journal = MigrationJournalV1 {
        schema: JOURNAL_SCHEMA.into(),
        version: VERSION,
        phase: "prepared".into(),
        source_existed,
        source_raw,
        source_digest,
        source,
        config,
        semantic_keys,
        light_files,
        config_preexisting,
        light_preexisting,
        marker,
        state_digest: String::new(),
    };
    validate_conservation(&journal)?;
    journal.state_digest = journal_digest(&journal)?;
    Ok(journal)
}

fn validate_marker(marker: &BootKvMigrationMarkerV1) -> M1ndResult<()> {
    if marker.schema != MIGRATION_SCHEMA
        || marker.version != VERSION
        || marker.status != "committed"
        || marker.source_path != LEGACY_BOOT_KV_FILE
        || marker.light_count != marker.light_digests.len()
        || !is_sha256(&marker.source_digest)
        || !is_sha256(&marker.conservation_digest)
        || !is_sha256(&marker.config_digest)
        || marker
            .light_digests
            .iter()
            .any(|(path, digest)| !safe_migration_light_path(path) || !is_sha256(digest))
        || marker.state_digest != marker_digest(marker)?
    {
        return Err(M1ndError::CorruptState {
            reason: "Boot KV migration marker schema/version/status/digest invalid".into(),
        });
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_migration_light_path(value: &str) -> bool {
    let path = Path::new(value);
    value.starts_with("agent-memory/")
        && value.ends_with(".light.md")
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn validate_journal(journal: &MigrationJournalV1) -> M1ndResult<()> {
    if journal.schema != JOURNAL_SCHEMA
        || journal.version != VERSION
        || journal.state_digest != journal_digest(journal)?
    {
        return Err(M1ndError::CorruptState {
            reason: "Boot KV migration journal schema/version/digest invalid".into(),
        });
    }
    validate_marker(&journal.marker)?;
    validate_conservation(journal)
}

fn validate_conservation(journal: &MigrationJournalV1) -> M1ndResult<()> {
    validate_entry_map(&journal.source)?;
    let source = sorted_entries(&journal.source);
    if !matches!(
        journal.phase.as_str(),
        "prepared" | "config_installed" | "targets_installed" | "source_retired" | "committed"
    ) {
        return Err(M1ndError::CorruptState {
            reason: format!("unknown Boot KV migration phase '{}'", journal.phase),
        });
    }
    let expected_source_digest = if journal.source_existed {
        let decoded: BootMemoryState = serde_json::from_str(&journal.source_raw)?;
        if decoded != journal.source {
            return Err(M1ndError::CorruptState {
                reason: "Boot KV journal source bytes differ from parsed source state".into(),
            });
        }
        sha256(journal.source_raw.as_bytes())
    } else {
        if !journal.source_raw.is_empty() || !journal.source.entries.is_empty() {
            return Err(M1ndError::CorruptState {
                reason: "absent Boot KV source carries impossible source bytes or entries".into(),
            });
        }
        sha256(b"<absent>")
    };

    let mut expected_config = BTreeMap::new();
    let mut expected_semantic_keys = Vec::new();
    let mut expected_light_files = BTreeMap::new();
    for (key, entry) in &source {
        if is_explicit_config(entry) {
            expected_config.insert(key.clone(), entry.clone());
        } else {
            expected_semantic_keys.push(key.clone());
            expected_light_files.insert(light_relative_path(entry)?, render_light(entry)?);
        }
    }
    expected_semantic_keys.sort();
    let config_bytes = serde_json::to_vec_pretty(&journal.config)?;
    let expected_light_digests = expected_light_files
        .iter()
        .map(|(path, contents)| (path.clone(), sha256(contents.as_bytes())))
        .collect::<BTreeMap<_, _>>();
    if journal.source_digest != expected_source_digest
        || journal.marker.source_existed != journal.source_existed
        || journal.marker.source_digest != expected_source_digest
        || journal.config.schema != CONFIG_SCHEMA
        || journal.config.version != VERSION
        || journal.config.entries != expected_config
        || journal.semantic_keys != expected_semantic_keys
        || journal.light_files != expected_light_files
        || journal.marker.source_count != source.len()
        || journal.marker.config_count != expected_config.len()
        || journal.marker.light_count != expected_semantic_keys.len()
        || journal.marker.conservation_digest != conservation_digest(&source)?
        || journal.marker.config_digest != sha256(&config_bytes)
        || journal.marker.light_digests != expected_light_digests
    {
        return Err(M1ndError::CorruptState {
            reason: "Boot KV migration plan violates entry-set conservation".into(),
        });
    }
    Ok(())
}

fn read_marker(root: &Path) -> M1ndResult<Option<BootKvMigrationMarkerV1>> {
    let path = root.join(MIGRATION_MARKER_FILE);
    let bytes = match read_optional_regular_file(&path)? {
        Some(bytes) => bytes,
        None => return Ok(None),
    };
    let marker: BootKvMigrationMarkerV1 = serde_json::from_slice(&bytes)?;
    validate_marker(&marker)?;
    Ok(Some(marker))
}

fn read_journal(root: &Path) -> M1ndResult<Option<MigrationJournalV1>> {
    let path = root.join(MIGRATION_JOURNAL_FILE);
    let bytes = match read_optional_regular_file(&path)? {
        Some(bytes) => bytes,
        None => return Ok(None),
    };
    let journal: MigrationJournalV1 = serde_json::from_slice(&bytes)?;
    validate_journal(&journal)?;
    Ok(Some(journal))
}

fn write_journal(root: &Path, journal: &mut MigrationJournalV1, phase: &str) -> M1ndResult<()> {
    journal.phase = phase.into();
    journal.state_digest = journal_digest(journal)?;
    write_json(&root.join(MIGRATION_JOURNAL_FILE), journal)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FaultPoint {
    JournalWritten,
    ConfigInstalled,
    LightsInstalled,
    SourceRetired,
    MarkerPublished,
}

fn migrate_with_fault(
    root: &Path,
    fault: Option<FaultPoint>,
) -> M1ndResult<BootKvMigrationMarkerV1> {
    if let Some(marker) = read_marker(root)? {
        verify_committed(root, &marker)?;
        if let Some(mut journal) = read_journal(root)? {
            if journal.marker != marker {
                return Err(M1ndError::CorruptState {
                    reason: "Boot KV committed marker differs from its migration journal".into(),
                });
            }
            if journal.phase != "committed" {
                write_journal(root, &mut journal, "committed")?;
            }
        }
        return Ok(marker);
    }
    let mut journal = match read_journal(root)? {
        Some(journal) => journal,
        None => {
            let mut journal = build_plan(root)?;
            write_journal(root, &mut journal, "prepared")?;
            journal
        }
    };
    if fault == Some(FaultPoint::JournalWritten) {
        return Err(M1ndError::PersistenceFailed("fault after journal".into()));
    }

    let config_bytes = serde_json::to_vec_pretty(&journal.config)?;
    if !existing_matches(&root.join(BOOT_CONFIG_FILE), &config_bytes)? {
        durable_atomic_write(&root.join(BOOT_CONFIG_FILE), &config_bytes)?;
    }
    write_journal(root, &mut journal, "config_installed")?;
    if fault == Some(FaultPoint::ConfigInstalled) {
        return Err(M1ndError::PersistenceFailed("fault after config".into()));
    }

    for (relative, contents) in &journal.light_files {
        let path = root.join(relative);
        if !existing_matches(&path, contents.as_bytes())? {
            durable_atomic_write(&path, contents.as_bytes())?;
        }
    }
    write_journal(root, &mut journal, "targets_installed")?;
    if fault == Some(FaultPoint::LightsInstalled) {
        return Err(M1ndError::PersistenceFailed("fault after lights".into()));
    }

    verify_targets(root, &journal.marker)?;
    write_json(&root.join(LEGACY_BOOT_KV_FILE), &BootMemoryState::default())?;
    write_journal(root, &mut journal, "source_retired")?;
    if fault == Some(FaultPoint::SourceRetired) {
        return Err(M1ndError::PersistenceFailed(
            "fault after source retirement".into(),
        ));
    }

    write_json(&root.join(MIGRATION_MARKER_FILE), &journal.marker)?;
    if fault == Some(FaultPoint::MarkerPublished) {
        return Err(M1ndError::PersistenceFailed("fault after marker".into()));
    }
    write_journal(root, &mut journal, "committed")?;
    verify_committed(root, &journal.marker)?;
    Ok(journal.marker)
}

fn verify_targets(root: &Path, marker: &BootKvMigrationMarkerV1) -> M1ndResult<()> {
    let config = read_required_regular_file(&root.join(BOOT_CONFIG_FILE))?;
    if sha256(&config) != marker.config_digest {
        return Err(M1ndError::CorruptState {
            reason: "migrated Boot Config digest mismatch".into(),
        });
    }
    for (relative, expected) in &marker.light_digests {
        let bytes = read_required_regular_file(&root.join(relative))?;
        if sha256(&bytes) != *expected {
            return Err(M1ndError::CorruptState {
                reason: format!("migrated L1GHT digest mismatch: {relative}"),
            });
        }
    }
    Ok(())
}

fn verify_committed(root: &Path, marker: &BootKvMigrationMarkerV1) -> M1ndResult<()> {
    validate_marker(marker)?;
    verify_targets(root, marker)?;
    let legacy: BootMemoryState = serde_json::from_slice(&read_required_regular_file(
        &root.join(LEGACY_BOOT_KV_FILE),
    )?)?;
    if !legacy.entries.is_empty() {
        return Err(M1ndError::CorruptState {
            reason: "retired Boot KV source became writable/non-empty after migration".into(),
        });
    }
    Ok(())
}

/// Idempotently migrate or recover an interrupted migration to the committed
/// state. A valid committed marker makes this a verification-only operation.
pub fn migrate_boot_kv(root: &Path) -> M1ndResult<BootKvMigrationMarkerV1> {
    migrate_with_fault(root, None)
}

/// Strict marker read for compatibility surfaces. `None` means the legacy
/// store has not been retired (e.g. a read-only process over an old runtime).
pub fn migration_status(root: &Path) -> M1ndResult<Option<Value>> {
    let Some(marker) = read_marker(root)? else {
        return Ok(None);
    };
    verify_committed(root, &marker)?;
    let journal = read_journal(root)?.ok_or_else(|| M1ndError::CorruptState {
        reason: "committed Boot KV migration marker has no durable journal".into(),
    })?;
    if journal.marker != marker || journal.phase != "committed" {
        return Err(M1ndError::CorruptState {
            reason: "committed Boot KV marker/journal pair is inconsistent".into(),
        });
    }
    Ok(Some(json!({
        "schema": marker.schema,
        "status": marker.status,
        "legacy_store": marker.source_path,
        "source_count": marker.source_count,
        "config_count": marker.config_count,
        "light_count": marker.light_count,
        "conservation_digest": marker.conservation_digest,
        "committed_at_ms": marker.committed_at_ms,
        "writes_retired": true,
        "config_target": BOOT_CONFIG_FILE,
        "semantic_target": "agent-memory/*.light.md",
    })))
}

fn read_optional_regular_file(path: &Path) -> M1ndResult<Option<Vec<u8>>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(M1ndError::CorruptState {
            reason: format!(
                "Boot KV checkpoint input '{}' is not a regular no-follow file",
                path.display()
            ),
        });
    }
    Ok(Some(std::fs::read(path)?))
}

fn read_required_regular_file(path: &Path) -> M1ndResult<Vec<u8>> {
    read_optional_regular_file(path)?.ok_or_else(|| M1ndError::CorruptState {
        reason: format!("required Boot KV file '{}' is missing", path.display()),
    })
}

/// Capture the complete Boot KV migration ownership set once, while the
/// session is initialized under its writer lease. Candidate checkpoints later
/// serialize this validated value directly from `SessionState`; they do not
/// race working-file re-reads or discover dynamic paths after mutation.
pub fn checkpoint_inventory(root: &Path) -> M1ndResult<BootKvCheckpointInventoryV1> {
    let mut fixed_files = BTreeMap::from([
        (LEGACY_BOOT_KV_FILE.to_string(), None),
        (BOOT_CONFIG_FILE.to_string(), None),
        (MIGRATION_MARKER_FILE.to_string(), None),
        (MIGRATION_JOURNAL_FILE.to_string(), None),
    ]);

    if let Some(marker) = read_marker(root)? {
        verify_committed(root, &marker)?;
        let journal = read_journal(root)?.ok_or_else(|| M1ndError::CorruptState {
            reason: "committed Boot KV migration has no checkpoint journal".into(),
        })?;
        if journal.marker != marker || journal.phase != "committed" {
            return Err(M1ndError::CorruptState {
                reason: "Boot KV checkpoint inventory refused inconsistent marker/journal".into(),
            });
        }

        fixed_files.insert(
            LEGACY_BOOT_KV_FILE.to_string(),
            Some(serde_json::to_vec_pretty(&BootMemoryState::default())?),
        );
        fixed_files.insert(
            BOOT_CONFIG_FILE.to_string(),
            Some(serde_json::to_vec_pretty(&journal.config)?),
        );
        fixed_files.insert(
            MIGRATION_MARKER_FILE.to_string(),
            Some(serde_json::to_vec_pretty(&journal.marker)?),
        );
        fixed_files.insert(
            MIGRATION_JOURNAL_FILE.to_string(),
            Some(serde_json::to_vec_pretty(&journal)?),
        );
        let migrated_lights = journal
            .light_files
            .into_iter()
            .map(|(path, contents)| (path, contents.into_bytes()))
            .collect();
        return Ok(BootKvCheckpointInventoryV1 {
            fixed_files,
            migrated_lights,
        });
    }

    // A marker-less journal is an interrupted migration, not a valid legacy
    // generation. Writable boot normally rolls it forward before this capture;
    // a read-only attach must not package the partial plan as authoritative.
    if read_journal(root)?.is_some() {
        return Err(M1ndError::CorruptState {
            reason: "Boot KV checkpoint inventory found an uncommitted migration journal".into(),
        });
    }

    if let Some(bytes) = read_optional_regular_file(&root.join(LEGACY_BOOT_KV_FILE))? {
        let state: BootMemoryState = serde_json::from_slice(&bytes)?;
        validate_entry_map(&state)?;
        fixed_files.insert(LEGACY_BOOT_KV_FILE.to_string(), Some(bytes));
    }
    if let Some(bytes) = read_optional_regular_file(&root.join(BOOT_CONFIG_FILE))? {
        let state: BootConfigStateV1 = serde_json::from_slice(&bytes)?;
        if state.schema != CONFIG_SCHEMA || state.version != VERSION {
            return Err(M1ndError::CorruptState {
                reason: "unsupported marker-less Boot Config checkpoint input".into(),
            });
        }
        fixed_files.insert(BOOT_CONFIG_FILE.to_string(), Some(bytes));
    }

    Ok(BootKvCheckpointInventoryV1 {
        fixed_files,
        migrated_lights: BTreeMap::new(),
    })
}

/// Read-only compatibility projection of every retired key. Values come from
/// the digest-validated original journal and name their active destination;
/// callers can keep reading during migration without reviving a write path.
pub fn compatibility_entries(root: &Path) -> M1ndResult<Option<Vec<MigratedCompatibilityEntry>>> {
    let Some(marker) = read_marker(root)? else {
        return Ok(None);
    };
    verify_committed(root, &marker)?;
    let journal = read_journal(root)?.ok_or_else(|| M1ndError::CorruptState {
        reason: "committed Boot KV migration marker has no compatibility journal".into(),
    })?;
    if journal.marker != marker || journal.phase != "committed" {
        return Err(M1ndError::CorruptState {
            reason: "Boot KV compatibility projection refused inconsistent marker/journal".into(),
        });
    }
    let mut rows = Vec::with_capacity(journal.source.entries.len());
    for (_, entry) in sorted_entries(&journal.source) {
        let (storage, target) = if is_explicit_config(&entry) {
            ("migrated_config", BOOT_CONFIG_FILE.to_string())
        } else {
            ("migrated_light", light_relative_path(&entry)?)
        };
        rows.push(MigratedCompatibilityEntry {
            entry,
            storage: storage.into(),
            target,
        });
    }
    Ok(Some(rows))
}

/// Content-addressed checkpoint inputs for migrated semantic entries. The
/// marker/config/journal are ordinary session sidecars; these files live under
/// `agent-memory/` and must travel with the checkpoint or a restored marker
/// would reference missing targets and correctly refuse boot.
pub fn checkpoint_light_artifacts(root: &Path) -> M1ndResult<Vec<(String, Vec<u8>)>> {
    let Some(marker) = read_marker(root)? else {
        return Ok(Vec::new());
    };
    verify_committed(root, &marker)?;
    marker
        .light_digests
        .keys()
        .map(|relative| {
            Ok((
                relative.clone(),
                read_required_regular_file(&root.join(relative))?,
            ))
        })
        .collect()
}

/// Restore exact pre-migration source bytes and remove only outputs that this
/// migration created. Any post-migration target mutation blocks rollback rather
/// than deleting foreign data.
pub fn rollback_boot_kv_migration(root: &Path) -> M1ndResult<()> {
    let journal = read_journal(root)?.ok_or_else(|| {
        M1ndError::PersistenceFailed(
            "Boot KV rollback requires the durable migration journal".into(),
        )
    })?;
    if let Some(marker) = read_marker(root)? {
        if marker != journal.marker {
            return Err(M1ndError::CorruptState {
                reason: "Boot KV rollback marker differs from durable journal".into(),
            });
        }
    }
    verify_targets(root, &journal.marker)?;

    if journal.source_existed {
        durable_atomic_write(
            &root.join(LEGACY_BOOT_KV_FILE),
            journal.source_raw.as_bytes(),
        )?;
    } else {
        let _ = std::fs::remove_file(root.join(LEGACY_BOOT_KV_FILE));
    }
    if !journal.config_preexisting {
        std::fs::remove_file(root.join(BOOT_CONFIG_FILE))?;
    }
    for relative in journal.light_files.keys() {
        if !journal.light_preexisting.contains(relative) {
            std::fs::remove_file(root.join(relative))?;
        }
    }
    match std::fs::remove_file(root.join(MIGRATION_MARKER_FILE)) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    std::fs::remove_file(root.join(MIGRATION_JOURNAL_FILE))?;
    // Windows refuses fsync on directory handles; write-through covers renames.
    #[cfg(not(windows))]
    std::fs::File::open(root)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(key: &str, value: Value, tags: &[&str]) -> BootMemoryEntry {
        BootMemoryEntry {
            key: key.into(),
            value,
            tags: tags.iter().map(|value| value.to_string()).collect(),
            source_refs: vec!["docs/source.md#truth".into()],
            updated_at_ms: 42,
            updated_by_agent: "legacy-agent".into(),
        }
    }

    fn seeded_root() -> (tempfile::TempDir, Vec<u8>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = BootMemoryState {
            entries: [
                (
                    "config.timeout".into(),
                    entry("config.timeout", json!(30), &["config"]),
                ),
                (
                    "architecture.doctrine".into(),
                    entry(
                        "architecture.doctrine",
                        json!({"rule": "fail closed"}),
                        &["boot"],
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&state).expect("encode source");
        std::fs::write(dir.path().join(LEGACY_BOOT_KV_FILE), &bytes).expect("seed");
        (dir, bytes)
    }

    #[test]
    fn migration_conserves_entries_restarts_and_retires_writer() {
        let (dir, _source) = seeded_root();
        let marker = migrate_boot_kv(dir.path()).expect("migrate");
        assert_eq!(marker.source_count, 2);
        assert_eq!(marker.config_count, 1);
        assert_eq!(marker.light_count, 1);
        let config: BootConfigStateV1 = serde_json::from_slice(
            &std::fs::read(dir.path().join(BOOT_CONFIG_FILE)).expect("config"),
        )
        .expect("decode config");
        assert!(config.entries.contains_key("config.timeout"));
        let retired: BootMemoryState = serde_json::from_slice(
            &std::fs::read(dir.path().join(LEGACY_BOOT_KV_FILE)).expect("retired"),
        )
        .expect("decode retired");
        assert!(retired.entries.is_empty());
        assert_eq!(migrate_boot_kv(dir.path()).expect("restart"), marker);
        assert!(migration_status(dir.path()).expect("status").is_some());
        let compatibility = compatibility_entries(dir.path())
            .expect("compatibility read")
            .expect("retired store");
        assert_eq!(compatibility.len(), 2);
        assert_eq!(compatibility[0].entry.key, "architecture.doctrine");
        assert_eq!(compatibility[0].storage, "migrated_light");
        assert_eq!(compatibility[1].entry.key, "config.timeout");
        assert_eq!(compatibility[1].storage, "migrated_config");
        let checkpoint_lights = checkpoint_light_artifacts(dir.path()).expect("checkpoint lights");
        assert_eq!(checkpoint_lights.len(), 1);
        assert_eq!(checkpoint_lights[0].0, compatibility[0].target);
        assert_eq!(
            sha256(&checkpoint_lights[0].1),
            marker.light_digests[&checkpoint_lights[0].0]
        );
        let inventory = checkpoint_inventory(dir.path()).expect("checkpoint inventory");
        for fixed in [
            LEGACY_BOOT_KV_FILE,
            BOOT_CONFIG_FILE,
            MIGRATION_MARKER_FILE,
            MIGRATION_JOURNAL_FILE,
        ] {
            assert!(inventory
                .fixed_file(fixed)
                .expect("fixed decision")
                .is_some());
        }
        let in_memory_lights = inventory.migrated_lights().collect::<Vec<_>>();
        assert_eq!(in_memory_lights.len(), 1);
        assert_eq!(in_memory_lights[0].0, checkpoint_lights[0].0);
        assert_eq!(in_memory_lights[0].1, checkpoint_lights[0].1);
    }

    #[test]
    fn every_commit_boundary_recovers_idempotently_old_or_new() {
        for fault in [
            FaultPoint::JournalWritten,
            FaultPoint::ConfigInstalled,
            FaultPoint::LightsInstalled,
            FaultPoint::SourceRetired,
            FaultPoint::MarkerPublished,
        ] {
            let (dir, _) = seeded_root();
            assert!(migrate_with_fault(dir.path(), Some(fault)).is_err());
            let marker = migrate_boot_kv(dir.path()).expect("recover forward");
            verify_committed(dir.path(), &marker).expect("committed valid");
        }
    }

    #[test]
    fn rollback_restores_source_bytes_exactly_and_is_conservative() {
        let (dir, source) = seeded_root();
        migrate_boot_kv(dir.path()).expect("migrate");
        rollback_boot_kv_migration(dir.path()).expect("rollback");
        assert_eq!(
            std::fs::read(dir.path().join(LEGACY_BOOT_KV_FILE)).expect("source restored"),
            source
        );
        assert!(!dir.path().join(BOOT_CONFIG_FILE).exists());
        assert!(!dir.path().join(MIGRATION_MARKER_FILE).exists());
    }

    #[test]
    fn corrupted_journal_and_post_migration_mutation_fail_closed() {
        let (dir, _) = seeded_root();
        assert!(migrate_with_fault(dir.path(), Some(FaultPoint::JournalWritten)).is_err());
        std::fs::write(dir.path().join(MIGRATION_JOURNAL_FILE), b"{}").expect("corrupt journal");
        assert!(migrate_boot_kv(dir.path()).is_err());

        let (dir, _) = seeded_root();
        migrate_boot_kv(dir.path()).expect("migrate");
        std::fs::write(dir.path().join(BOOT_CONFIG_FILE), b"foreign mutation")
            .expect("mutate target");
        assert!(rollback_boot_kv_migration(dir.path()).is_err());
    }

    #[test]
    fn an_empty_or_absent_legacy_store_is_still_explicitly_retired() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = migrate_boot_kv(dir.path()).expect("retire empty");
        assert_eq!(marker.source_count, 0);
        assert!(migration_status(dir.path()).expect("status").is_some());
    }
}
