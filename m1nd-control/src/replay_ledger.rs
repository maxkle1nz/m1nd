//! Replay protection contracts and a fail-closed durable file ledger.
//!
//! The trait is intentionally small so a runtime-owned database or secure
//! monotonic store can replace the file implementation without changing the
//! authority verifier. The file implementation acknowledges a claim only
//! after the append has been synchronized. Any ambiguous write poisons the
//! open handle; callers must reopen and replay the durable log before making
//! another authority decision.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{canonical_json_string, digest_canonical, CanonicalError};

pub const REPLAY_CLAIM_SCHEMA: &str = "m1nd-replay-claim-v1";
pub const REPLAY_LEDGER_RECORD_SCHEMA: &str = "m1nd-replay-ledger-record-v1";
pub const REPLAY_CLAIM_DIGEST_DOMAIN: &str = "m1nd-replay-claim-v1";
pub const REPLAY_SCOPE_DIGEST_DOMAIN: &str = "m1nd-replay-scope-v1";
pub const REPLAY_LEDGER_RECORD_DIGEST_DOMAIN: &str = "m1nd-replay-ledger-record-v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayClaimV1 {
    pub schema: String,
    pub namespace: String,
    pub issuer_subject_id: String,
    pub key_id: String,
    pub subject_id: String,
    pub nonce: String,
    pub object_digest: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

impl ReplayClaimV1 {
    pub fn claim_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(REPLAY_CLAIM_DIGEST_DOMAIN, self)
    }

    /// The replay scope deliberately excludes payload/object bytes. Reusing a
    /// nonce under the same namespace and issuer key is rejected even if an
    /// attacker changes the subject or payload and produces another signature.
    pub fn scope_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(
            REPLAY_SCOPE_DIGEST_DOMAIN,
            &ReplayScopeV1 {
                namespace: &self.namespace,
                issuer_subject_id: &self.issuer_subject_id,
                key_id: &self.key_id,
                nonce: &self.nonce,
            },
        )
    }

    fn validate_static(&self) -> Result<(), ReplayLedgerError> {
        if self.schema != REPLAY_CLAIM_SCHEMA {
            return Err(ReplayLedgerError::Schema {
                expected: REPLAY_CLAIM_SCHEMA,
                actual: self.schema.clone(),
            });
        }
        for (field, value) in [
            ("namespace", self.namespace.as_str()),
            ("issuer_subject_id", self.issuer_subject_id.as_str()),
            ("key_id", self.key_id.as_str()),
            ("subject_id", self.subject_id.as_str()),
            ("nonce", self.nonce.as_str()),
            ("object_digest", self.object_digest.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(ReplayLedgerError::EmptyRequired { field });
            }
        }
        if self.expires_at <= self.issued_at {
            return Err(ReplayLedgerError::InvalidTimeOrder {
                issued_at: self.issued_at,
                expires_at: self.expires_at,
            });
        }
        Ok(())
    }

    pub fn validate_at(
        &self,
        now_ms: u64,
        max_future_clock_skew_ms: u64,
    ) -> Result<(), ReplayLedgerError> {
        self.validate_static()?;
        let latest_allowed = now_ms.saturating_add(max_future_clock_skew_ms);
        if self.issued_at > latest_allowed {
            return Err(ReplayLedgerError::IssuedInFuture {
                issued_at: self.issued_at,
                latest_allowed,
            });
        }
        if now_ms >= self.expires_at {
            return Err(ReplayLedgerError::Expired {
                expires_at: self.expires_at,
                now_ms,
            });
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct ReplayScopeV1<'a> {
    namespace: &'a str,
    issuer_subject_id: &'a str,
    key_id: &'a str,
    nonce: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayDurability {
    Volatile,
    SyncedFile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayReceiptV1 {
    pub sequence: u64,
    pub claim_digest: String,
    pub scope_digest: String,
    pub durability: ReplayDurability,
}

pub trait ReplayLedger {
    fn consume(
        &mut self,
        claim: &ReplayClaimV1,
        now_ms: u64,
        max_future_clock_skew_ms: u64,
    ) -> Result<ReplayReceiptV1, ReplayLedgerError>;
}

#[derive(Debug, Error)]
pub enum ReplayLedgerError {
    #[error("unsupported replay claim schema '{actual}', expected '{expected}'")]
    Schema {
        expected: &'static str,
        actual: String,
    },
    #[error("required replay claim field '{field}' is empty")]
    EmptyRequired { field: &'static str },
    #[error("replay claim expires_at ({expires_at}) must be later than issued_at ({issued_at})")]
    InvalidTimeOrder { issued_at: u64, expires_at: u64 },
    #[error("replay claim issued_at {issued_at} exceeds permitted future time {latest_allowed}")]
    IssuedInFuture { issued_at: u64, latest_allowed: u64 },
    #[error("replay claim expired at {expires_at}; validation time is {now_ms}")]
    Expired { expires_at: u64, now_ms: u64 },
    #[error("replay detected for scope {scope_digest}")]
    Replay { scope_digest: String },
    #[error("replay ledger path is a symbolic link: {path}")]
    SymlinkRefused { path: PathBuf },
    #[error("replay ledger changed outside this handle (expected {expected_len} bytes, observed {observed_len})")]
    ConcurrentModification {
        expected_len: u64,
        observed_len: u64,
    },
    #[error("replay ledger handle is poisoned after an ambiguous durable write")]
    Poisoned,
    #[error("corrupt replay ledger at line {line}: {reason}")]
    Corrupt { line: usize, reason: String },
    #[error("replay ledger I/O failed during {operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Canonical(#[from] CanonicalError),
}

pub struct MemoryReplayLedger {
    consumed_scopes: BTreeSet<String>,
    next_sequence: u64,
}

impl Default for MemoryReplayLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryReplayLedger {
    pub fn new() -> Self {
        Self {
            consumed_scopes: BTreeSet::new(),
            next_sequence: 1,
        }
    }

    pub fn consumed_count(&self) -> usize {
        self.consumed_scopes.len()
    }
}

impl ReplayLedger for MemoryReplayLedger {
    fn consume(
        &mut self,
        claim: &ReplayClaimV1,
        now_ms: u64,
        max_future_clock_skew_ms: u64,
    ) -> Result<ReplayReceiptV1, ReplayLedgerError> {
        claim.validate_at(now_ms, max_future_clock_skew_ms)?;
        let scope_digest = claim.scope_digest()?;
        if self.consumed_scopes.contains(&scope_digest) {
            return Err(ReplayLedgerError::Replay { scope_digest });
        }
        let receipt = ReplayReceiptV1 {
            sequence: self.next_sequence,
            claim_digest: claim.claim_digest()?,
            scope_digest: scope_digest.clone(),
            durability: ReplayDurability::Volatile,
        };
        self.consumed_scopes.insert(scope_digest);
        self.next_sequence = self.next_sequence.saturating_add(1);
        Ok(receipt)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplayLedgerRecordV1 {
    schema: String,
    sequence: u64,
    previous_record_digest: Option<String>,
    claim: ReplayClaimV1,
    claim_digest: String,
    scope_digest: String,
    record_digest: String,
}

#[derive(Serialize)]
struct ReplayLedgerRecordMaterialV1<'a> {
    schema: &'a str,
    sequence: u64,
    previous_record_digest: Option<&'a str>,
    claim: &'a ReplayClaimV1,
    claim_digest: &'a str,
    scope_digest: &'a str,
}

impl ReplayLedgerRecordV1 {
    fn compute_record_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(
            REPLAY_LEDGER_RECORD_DIGEST_DOMAIN,
            &ReplayLedgerRecordMaterialV1 {
                schema: &self.schema,
                sequence: self.sequence,
                previous_record_digest: self.previous_record_digest.as_deref(),
                claim: &self.claim,
                claim_digest: &self.claim_digest,
                scope_digest: &self.scope_digest,
            },
        )
    }
}

/// Durable single-writer implementation.
///
/// It detects length drift from another writer and then poisons the handle,
/// but it is not an inter-process lock. A runtime with concurrent authority
/// workers must serialize this handle behind one owner or provide another
/// [`ReplayLedger`] implementation with transactional uniqueness.
pub struct PersistentReplayLedger {
    path: PathBuf,
    file: File,
    consumed_scopes: BTreeSet<String>,
    next_sequence: u64,
    tail_digest: Option<String>,
    known_len: u64,
    poisoned: bool,
}

impl PersistentReplayLedger {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ReplayLedgerError> {
        let path = path.as_ref().to_path_buf();
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() {
                return Err(ReplayLedgerError::SymlinkRefused { path });
            }
        }

        let parent = path.parent().filter(|value| !value.as_os_str().is_empty());
        if let Some(parent) = parent {
            fs::create_dir_all(parent).map_err(|source| ReplayLedgerError::Io {
                operation: "create_parent_directory",
                source,
            })?;
        }

        let existed = path.exists();
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)
            .map_err(|source| ReplayLedgerError::Io {
                operation: "open",
                source,
            })?;

        if !existed {
            file.sync_all().map_err(|source| ReplayLedgerError::Io {
                operation: "sync_new_file",
                source,
            })?;
            // Windows refuses fsync on directory handles (ACCESS_DENIED); the
            // new-file entry is made durable there by write-through semantics.
            #[cfg(not(windows))]
            if let Some(parent) = parent {
                File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|source| ReplayLedgerError::Io {
                        operation: "sync_parent_directory",
                        source,
                    })?;
            }
            #[cfg(windows)]
            let _ = parent;
        }

        let mut reader = file.try_clone().map_err(|source| ReplayLedgerError::Io {
            operation: "clone_for_replay",
            source,
        })?;
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|source| ReplayLedgerError::Io {
                operation: "read_replay_log",
                source,
            })?;

        let (consumed_scopes, next_sequence, tail_digest) = replay_records(&bytes)?;
        Ok(Self {
            path,
            file,
            consumed_scopes,
            next_sequence,
            tail_digest,
            known_len: bytes.len() as u64,
            poisoned: false,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn consumed_count(&self) -> usize {
        self.consumed_scopes.len()
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
}

impl ReplayLedger for PersistentReplayLedger {
    fn consume(
        &mut self,
        claim: &ReplayClaimV1,
        now_ms: u64,
        max_future_clock_skew_ms: u64,
    ) -> Result<ReplayReceiptV1, ReplayLedgerError> {
        if self.poisoned {
            return Err(ReplayLedgerError::Poisoned);
        }
        claim.validate_at(now_ms, max_future_clock_skew_ms)?;
        let claim_digest = claim.claim_digest()?;
        let scope_digest = claim.scope_digest()?;
        if self.consumed_scopes.contains(&scope_digest) {
            return Err(ReplayLedgerError::Replay { scope_digest });
        }

        let observed_len = self
            .file
            .metadata()
            .map_err(|source| ReplayLedgerError::Io {
                operation: "read_length_before_append",
                source,
            })?
            .len();
        if observed_len != self.known_len {
            self.poisoned = true;
            return Err(ReplayLedgerError::ConcurrentModification {
                expected_len: self.known_len,
                observed_len,
            });
        }

        let mut record = ReplayLedgerRecordV1 {
            schema: REPLAY_LEDGER_RECORD_SCHEMA.to_owned(),
            sequence: self.next_sequence,
            previous_record_digest: self.tail_digest.clone(),
            claim: claim.clone(),
            claim_digest: claim_digest.clone(),
            scope_digest: scope_digest.clone(),
            record_digest: String::new(),
        };
        record.record_digest = record.compute_record_digest()?;
        let mut encoded = canonical_json_string(&record)?.into_bytes();
        encoded.push(b'\n');

        if let Err(source) = self.file.write_all(&encoded) {
            self.poisoned = true;
            return Err(ReplayLedgerError::Io {
                operation: "append_record",
                source,
            });
        }
        if let Err(source) = self.file.sync_all() {
            self.poisoned = true;
            return Err(ReplayLedgerError::Io {
                operation: "sync_record",
                source,
            });
        }

        self.known_len = self.known_len.saturating_add(encoded.len() as u64);
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.tail_digest = Some(record.record_digest);
        self.consumed_scopes.insert(scope_digest.clone());
        Ok(ReplayReceiptV1 {
            sequence: record.sequence,
            claim_digest,
            scope_digest,
            durability: ReplayDurability::SyncedFile,
        })
    }
}

fn replay_records(
    bytes: &[u8],
) -> Result<(BTreeSet<String>, u64, Option<String>), ReplayLedgerError> {
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err(ReplayLedgerError::Corrupt {
            line: bytes.iter().filter(|byte| **byte == b'\n').count() + 1,
            reason: "unterminated or torn tail record".to_owned(),
        });
    }

    let mut consumed_scopes = BTreeSet::new();
    let mut expected_sequence = 1_u64;
    let mut tail_digest: Option<String> = None;
    let lines: Vec<&[u8]> = bytes.split(|byte| *byte == b'\n').collect();
    for (line_index, line) in lines.iter().copied().enumerate() {
        if line.is_empty() {
            if line_index + 1 == lines.len() {
                continue;
            }
            return Err(ReplayLedgerError::Corrupt {
                line: line_index + 1,
                reason: "blank record in durable ledger".to_owned(),
            });
        }
        let line_number = line_index + 1;
        let record: ReplayLedgerRecordV1 =
            serde_json::from_slice(line).map_err(|error| ReplayLedgerError::Corrupt {
                line: line_number,
                reason: format!("invalid JSON: {error}"),
            })?;
        let canonical = canonical_json_string(&record)?;
        if canonical.as_bytes() != line {
            return Err(ReplayLedgerError::Corrupt {
                line: line_number,
                reason: "record is not canonical JSON".to_owned(),
            });
        }
        if record.schema != REPLAY_LEDGER_RECORD_SCHEMA {
            return Err(ReplayLedgerError::Corrupt {
                line: line_number,
                reason: format!("unsupported record schema '{}'", record.schema),
            });
        }
        if record.sequence != expected_sequence {
            return Err(ReplayLedgerError::Corrupt {
                line: line_number,
                reason: format!(
                    "expected sequence {expected_sequence}, observed {}",
                    record.sequence
                ),
            });
        }
        if record.previous_record_digest != tail_digest {
            return Err(ReplayLedgerError::Corrupt {
                line: line_number,
                reason: "previous record digest does not match the durable tail".to_owned(),
            });
        }
        record
            .claim
            .validate_static()
            .map_err(|error| ReplayLedgerError::Corrupt {
                line: line_number,
                reason: format!("invalid replay claim: {error}"),
            })?;
        let expected_claim_digest = record.claim.claim_digest()?;
        if record.claim_digest != expected_claim_digest {
            return Err(ReplayLedgerError::Corrupt {
                line: line_number,
                reason: "claim digest mismatch".to_owned(),
            });
        }
        let expected_scope_digest = record.claim.scope_digest()?;
        if record.scope_digest != expected_scope_digest {
            return Err(ReplayLedgerError::Corrupt {
                line: line_number,
                reason: "scope digest mismatch".to_owned(),
            });
        }
        let expected_record_digest = record.compute_record_digest()?;
        if record.record_digest != expected_record_digest {
            return Err(ReplayLedgerError::Corrupt {
                line: line_number,
                reason: "record self-digest mismatch".to_owned(),
            });
        }
        if !consumed_scopes.insert(record.scope_digest.clone()) {
            return Err(ReplayLedgerError::Corrupt {
                line: line_number,
                reason: "duplicate replay scope in durable ledger".to_owned(),
            });
        }

        expected_sequence = expected_sequence.saturating_add(1);
        tail_digest = Some(record.record_digest);
    }
    Ok((consumed_scopes, expected_sequence, tail_digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(nonce: &str) -> ReplayClaimV1 {
        ReplayClaimV1 {
            schema: REPLAY_CLAIM_SCHEMA.to_owned(),
            namespace: "capability".to_owned(),
            issuer_subject_id: "owner-1".to_owned(),
            key_id: "key-1".to_owned(),
            subject_id: "agent-1".to_owned(),
            nonce: nonce.to_owned(),
            object_digest: "digest-1".to_owned(),
            issued_at: 1_000,
            expires_at: 10_000,
        }
    }

    #[test]
    fn memory_ledger_rejects_same_nonce_even_when_payload_changes() {
        let mut ledger = MemoryReplayLedger::new();
        ledger.consume(&claim("nonce-1"), 2_000, 50).unwrap();

        let mut changed_payload = claim("nonce-1");
        changed_payload.object_digest = "attacker-payload".to_owned();
        assert!(matches!(
            ledger.consume(&changed_payload, 2_000, 50),
            Err(ReplayLedgerError::Replay { .. })
        ));
        assert_eq!(ledger.consumed_count(), 1);
    }

    #[test]
    fn validation_rejects_future_expired_and_invalid_claims_before_consumption() {
        let mut ledger = MemoryReplayLedger::new();
        let mut future = claim("future");
        future.issued_at = 2_101;
        assert!(matches!(
            ledger.consume(&future, 2_000, 100),
            Err(ReplayLedgerError::IssuedInFuture { .. })
        ));

        let mut expired = claim("expired");
        expired.expires_at = 2_000;
        assert!(matches!(
            ledger.consume(&expired, 2_000, 100),
            Err(ReplayLedgerError::Expired { .. })
        ));

        let mut invalid = claim("invalid");
        invalid.expires_at = invalid.issued_at;
        assert!(matches!(
            ledger.consume(&invalid, 1_000, 100),
            Err(ReplayLedgerError::InvalidTimeOrder { .. })
        ));
        assert_eq!(ledger.consumed_count(), 0);
    }

    #[test]
    fn durable_replay_survives_drop_and_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("replay.jsonl");
        {
            let mut ledger = PersistentReplayLedger::open(&path).unwrap();
            let receipt = ledger.consume(&claim("restart"), 2_000, 50).unwrap();
            assert_eq!(receipt.durability, ReplayDurability::SyncedFile);
            assert_eq!(receipt.sequence, 1);
        }

        let mut reopened = PersistentReplayLedger::open(&path).unwrap();
        assert_eq!(reopened.consumed_count(), 1);
        assert!(matches!(
            reopened.consume(&claim("restart"), 2_000, 50),
            Err(ReplayLedgerError::Replay { .. })
        ));
        let second = reopened.consume(&claim("second"), 2_000, 50).unwrap();
        assert_eq!(second.sequence, 2);
    }

    #[test]
    fn torn_tail_fails_closed_on_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("replay.jsonl");
        {
            let mut ledger = PersistentReplayLedger::open(&path).unwrap();
            ledger.consume(&claim("first"), 2_000, 50).unwrap();
        }
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"schema\":\"torn").unwrap();
        file.sync_all().unwrap();

        assert!(matches!(
            PersistentReplayLedger::open(&path),
            Err(ReplayLedgerError::Corrupt { .. })
        ));
    }

    #[test]
    fn modified_durable_record_fails_closed_on_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("replay.jsonl");
        {
            let mut ledger = PersistentReplayLedger::open(&path).unwrap();
            ledger.consume(&claim("first"), 2_000, 50).unwrap();
        }
        let original = fs::read_to_string(&path).unwrap();
        let modified = original.replace("digest-1", "digest-X");
        fs::write(&path, modified).unwrap();

        assert!(matches!(
            PersistentReplayLedger::open(&path),
            Err(ReplayLedgerError::Corrupt { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_ledger_path_is_refused() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target.jsonl");
        fs::write(&target, b"").unwrap();
        let link = directory.path().join("ledger.jsonl");
        symlink(&target, &link).unwrap();

        assert!(matches!(
            PersistentReplayLedger::open(&link),
            Err(ReplayLedgerError::SymlinkRefused { .. })
        ));
    }
}
