use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs::File;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use m1nd_control::{
    AuthorityTransactionV1, AuthorityWalContractError, AuthorityWalPayloadV1, AuthorityWalPhase,
    AuthorityWalRecordV1,
};
use sha2::{Digest, Sha256};

use crate::light_author_handlers::LockGuard;
use crate::owner_authorization_broker::AuthorityWalCommitWitnessV1;
use crate::protected_journal_head::{
    advance_protected_head, verify_or_initialize_protected_head, ProtectedJournalHeadSnapshotV1,
    SharedProtectedJournalHeadBackendV1, AUTHORITY_WAL_HEAD_DOMAIN,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityWalRecoveryReport {
    pub record_count: u64,
    pub recovered_torn_tail: bool,
    pub truncated_tail_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityWalTerminalOutcome {
    pub transaction_id: String,
    pub idempotency_key: String,
    pub phase: AuthorityWalPhase,
    pub terminal_outcome_digest: String,
    pub terminal_record_digest: String,
    verified_commit_witness: Option<VerifiedAuthorityWalCommitWitnessV1>,
}

impl AuthorityWalTerminalOutcome {
    pub(crate) fn verified_commit_witness(&self) -> Option<VerifiedAuthorityWalCommitWitnessV1> {
        self.verified_commit_witness.clone()
    }
}

/// Opaque proof that the witness was derived from a record whose complete WAL
/// contract, chain, signer metadata, and signature were verified by
/// `AuthorityWal`. Other modules can inspect/consume it but cannot construct
/// one on a production build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedAuthorityWalCommitWitnessV1(AuthorityWalCommitWitnessV1);

impl VerifiedAuthorityWalCommitWitnessV1 {
    pub(crate) fn witness(&self) -> &AuthorityWalCommitWitnessV1 {
        &self.0
    }

    pub(crate) fn into_witness(self) -> AuthorityWalCommitWitnessV1 {
        self.0
    }

    #[cfg(test)]
    pub(crate) fn explicit_test_only(witness: AuthorityWalCommitWitnessV1) -> Self {
        Self(witness)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityWalCryptoAssurance {
    UnavailableFailClosed,
    SoftwareTestOnlyNotProven,
    ProductionCryptographic,
}

/// Injected owner-key boundary for AuthorityWAL records. Implementations sign
/// and verify the exact canonical record body *after* sequence and chain fields
/// have been assigned. The WAL never accepts phase signatures supplied by a
/// caller or by an authenticated request context.
pub trait AuthorityWalRecordCrypto: Send + Sync {
    fn assurance(&self) -> AuthorityWalCryptoAssurance;
    fn issuer(&self) -> &str;
    fn key_id(&self) -> &str;
    fn algorithm(&self) -> &str;
    fn sign(&self, canonical_record_message: &[u8]) -> Result<String, String>;
    fn verify(&self, canonical_record_message: &[u8], signature: &str) -> Result<(), String>;
}

struct UnavailableAuthorityWalRecordCrypto;

impl AuthorityWalRecordCrypto for UnavailableAuthorityWalRecordCrypto {
    fn assurance(&self) -> AuthorityWalCryptoAssurance {
        AuthorityWalCryptoAssurance::UnavailableFailClosed
    }

    fn issuer(&self) -> &str {
        "unavailable"
    }

    fn key_id(&self) -> &str {
        "unavailable"
    }

    fn algorithm(&self) -> &str {
        "UNAVAILABLE_FAIL_CLOSED"
    }

    fn sign(&self, _canonical_record_message: &[u8]) -> Result<String, String> {
        Err("no AuthorityWAL production signer is installed".to_string())
    }

    fn verify(&self, _canonical_record_message: &[u8], _signature: &str) -> Result<(), String> {
        Err("no AuthorityWAL production verifier is installed".to_string())
    }
}

/// Explicit deterministic backend for unit/integration batteries. The name,
/// algorithm, and assurance are intentionally impossible to mistake for a
/// production protected-key adapter.
pub struct SoftwareTestAuthorityWalRecordCrypto {
    secret: Vec<u8>,
}

impl SoftwareTestAuthorityWalRecordCrypto {
    pub fn explicit_not_production(secret: impl AsRef<[u8]>) -> Self {
        Self {
            secret: secret.as_ref().to_vec(),
        }
    }

    fn signature(&self, message: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"m1nd-wal-software-test-only-not-proven-v1\0");
        hasher.update((self.secret.len() as u64).to_be_bytes());
        hasher.update(&self.secret);
        hasher.update((message.len() as u64).to_be_bytes());
        hasher.update(message);
        crate::util::hex_lower(&hasher.finalize())
    }
}

impl AuthorityWalRecordCrypto for SoftwareTestAuthorityWalRecordCrypto {
    fn assurance(&self) -> AuthorityWalCryptoAssurance {
        AuthorityWalCryptoAssurance::SoftwareTestOnlyNotProven
    }

    fn issuer(&self) -> &str {
        "software-test-owner-not-production"
    }

    fn key_id(&self) -> &str {
        "software-test-wal-key-not-production"
    }

    fn algorithm(&self) -> &str {
        "SOFTWARE_TEST_SHA256_NOT_CRYPTOGRAPHIC"
    }

    fn sign(&self, canonical_record_message: &[u8]) -> Result<String, String> {
        Ok(self.signature(canonical_record_message))
    }

    fn verify(&self, canonical_record_message: &[u8], signature: &str) -> Result<(), String> {
        if self.signature(canonical_record_message) == signature {
            Ok(())
        } else {
            Err("software-test WAL signature mismatch".to_string())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthorityWalAppendOutcome {
    Appended {
        sequence: u64,
        phase: AuthorityWalPhase,
        record_digest: String,
        verified_commit_witness: Option<VerifiedAuthorityWalCommitWitnessV1>,
    },
    TerminalReplay(AuthorityWalTerminalOutcome),
}

#[derive(Debug)]
pub enum AuthorityWalError {
    Io(io::Error),
    Serialization(serde_json::Error),
    Contract(AuthorityWalContractError),
    Crypto {
        operation: &'static str,
        detail: String,
    },
    WriterLock(String),
    Corruption {
        offset: u64,
        detail: String,
    },
    SequenceMismatch {
        expected: u64,
        observed: u64,
    },
    HashChainMismatch {
        sequence: u64,
    },
    IllegalPhase {
        transaction_id: String,
        previous: Option<AuthorityWalPhase>,
        attempted: AuthorityWalPhase,
    },
    DuplicateTransaction {
        transaction_id: String,
    },
    DuplicateIdempotency {
        idempotency_key: String,
    },
    IdempotencyConflict {
        idempotency_key: String,
    },
    ProtectedHead {
        detail: String,
    },
    Poisoned,
}

impl fmt::Display for AuthorityWalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "AuthorityWAL I/O error: {error}"),
            Self::Serialization(error) => {
                write!(formatter, "AuthorityWAL serialization error: {error}")
            }
            Self::Contract(error) => write!(formatter, "AuthorityWAL contract error: {error}"),
            Self::Crypto { operation, detail } => {
                write!(
                    formatter,
                    "AuthorityWAL crypto {operation} failed: {detail}"
                )
            }
            Self::WriterLock(error) => {
                write!(formatter, "AuthorityWAL writer lock failed: {error}")
            }
            Self::Corruption { offset, detail } => {
                write!(
                    formatter,
                    "AuthorityWAL corruption at byte {offset}: {detail}"
                )
            }
            Self::SequenceMismatch { expected, observed } => write!(
                formatter,
                "AuthorityWAL sequence mismatch: expected {expected}, observed {observed}"
            ),
            Self::HashChainMismatch { sequence } => write!(
                formatter,
                "AuthorityWAL previous-record hash mismatch at sequence {sequence}"
            ),
            Self::IllegalPhase {
                transaction_id,
                previous,
                attempted,
            } => write!(
                formatter,
                "illegal AuthorityWAL phase for {transaction_id}: {previous:?} -> {attempted:?}"
            ),
            Self::DuplicateTransaction { transaction_id } => {
                write!(
                    formatter,
                    "duplicate AuthorityWAL transaction '{transaction_id}'"
                )
            }
            Self::DuplicateIdempotency { idempotency_key } => write!(
                formatter,
                "AuthorityWAL idempotency key '{idempotency_key}' is already in flight"
            ),
            Self::IdempotencyConflict { idempotency_key } => write!(
                formatter,
                "AuthorityWAL idempotency key '{idempotency_key}' binds a different intent"
            ),
            Self::ProtectedHead { detail } => {
                write!(formatter, "AuthorityWAL protected head: {detail}")
            }
            Self::Poisoned => formatter.write_str(
                "AuthorityWAL writer is poisoned after an uncertain write; drop and reopen it",
            ),
        }
    }
}

impl Error for AuthorityWalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Serialization(error) => Some(error),
            Self::Contract(error) => Some(error),
            Self::WriterLock(_)
            | Self::Crypto { .. }
            | Self::Corruption { .. }
            | Self::SequenceMismatch { .. }
            | Self::HashChainMismatch { .. }
            | Self::IllegalPhase { .. }
            | Self::DuplicateTransaction { .. }
            | Self::DuplicateIdempotency { .. }
            | Self::IdempotencyConflict { .. }
            | Self::ProtectedHead { .. }
            | Self::Poisoned => None,
        }
    }
}

impl From<io::Error> for AuthorityWalError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for AuthorityWalError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error)
    }
}

impl From<AuthorityWalContractError> for AuthorityWalError {
    fn from(error: AuthorityWalContractError) -> Self {
        Self::Contract(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct IdempotencyScope {
    organism_id: String,
    brain_id: String,
    subject_id: String,
    action_id: String,
    idempotency_key: String,
}

impl IdempotencyScope {
    fn from_transaction(transaction: &AuthorityTransactionV1) -> Self {
        let binding = transaction.binding();
        Self {
            organism_id: binding.organism_id.clone(),
            brain_id: binding.brain_id.clone(),
            subject_id: binding.subject_id.clone(),
            action_id: binding.action_id.clone(),
            idempotency_key: binding.idempotency_key.clone(),
        }
    }
}

#[derive(Clone, Debug)]
struct ReplayTransaction {
    transaction: AuthorityTransactionV1,
    phase: AuthorityWalPhase,
    terminal: Option<AuthorityWalTerminalOutcome>,
}

#[derive(Clone, Debug, Default)]
struct ReplayIndex {
    record_count: u64,
    last_record_digest: Option<String>,
    transactions: HashMap<String, ReplayTransaction>,
    idempotency: HashMap<IdempotencyScope, String>,
}

impl ReplayIndex {
    fn apply_record(&mut self, record: &AuthorityWalRecordV1) -> Result<(), AuthorityWalError> {
        match &record.payload {
            AuthorityWalPayloadV1::Prepare(payload) => {
                record.validate_against_transaction(&payload.transaction)?;
                if self.transactions.contains_key(&record.transaction_id) {
                    return Err(AuthorityWalError::DuplicateTransaction {
                        transaction_id: record.transaction_id.clone(),
                    });
                }
                let scope = IdempotencyScope::from_transaction(&payload.transaction);
                if self.idempotency.contains_key(&scope) {
                    return Err(AuthorityWalError::DuplicateIdempotency {
                        idempotency_key: record.idempotency_key.clone(),
                    });
                }
                self.idempotency
                    .insert(scope, record.transaction_id.clone());
                self.transactions.insert(
                    record.transaction_id.clone(),
                    ReplayTransaction {
                        transaction: payload.as_ref().transaction.clone(),
                        phase: AuthorityWalPhase::Prepare,
                        terminal: None,
                    },
                );
            }
            AuthorityWalPayloadV1::Provisional(_) => {
                self.advance(record, AuthorityWalPhase::Prepare)?;
            }
            AuthorityWalPayloadV1::Commit(payload) => {
                self.advance(record, AuthorityWalPhase::Provisional)?;
                let replay = self
                    .transactions
                    .get_mut(&record.transaction_id)
                    .expect("advance proved transaction exists");
                let verified_commit_witness =
                    VerifiedAuthorityWalCommitWitnessV1(AuthorityWalCommitWitnessV1 {
                        transaction_id: record.transaction_id.clone(),
                        phase: AuthorityWalPhase::Commit,
                        transaction_digest: replay.transaction.transaction_digest().to_string(),
                        authorization_snapshot_digest: payload
                            .authorization_snapshot_digest
                            .clone(),
                        terminal_record_digest: record.record_digest.clone(),
                        committed_at: payload.committed_at,
                    });
                replay.terminal = Some(AuthorityWalTerminalOutcome {
                    transaction_id: record.transaction_id.clone(),
                    idempotency_key: record.idempotency_key.clone(),
                    phase: AuthorityWalPhase::Commit,
                    terminal_outcome_digest: payload.terminal_outcome_digest.clone(),
                    terminal_record_digest: record.record_digest.clone(),
                    verified_commit_witness: Some(verified_commit_witness),
                });
            }
            AuthorityWalPayloadV1::Abort(payload) => {
                let previous = self
                    .transactions
                    .get(&record.transaction_id)
                    .map(|replay| replay.phase);
                if !matches!(
                    previous,
                    Some(AuthorityWalPhase::Prepare | AuthorityWalPhase::Provisional)
                ) {
                    return Err(AuthorityWalError::IllegalPhase {
                        transaction_id: record.transaction_id.clone(),
                        previous,
                        attempted: AuthorityWalPhase::Abort,
                    });
                }
                let transaction = &self
                    .transactions
                    .get(&record.transaction_id)
                    .expect("phase match proved transaction exists")
                    .transaction;
                record.validate_against_transaction(transaction)?;
                let replay = self
                    .transactions
                    .get_mut(&record.transaction_id)
                    .expect("phase match proved transaction exists");
                replay.phase = AuthorityWalPhase::Abort;
                replay.terminal = Some(AuthorityWalTerminalOutcome {
                    transaction_id: record.transaction_id.clone(),
                    idempotency_key: record.idempotency_key.clone(),
                    phase: AuthorityWalPhase::Abort,
                    terminal_outcome_digest: payload.terminal_outcome_digest.clone(),
                    terminal_record_digest: record.record_digest.clone(),
                    verified_commit_witness: None,
                });
            }
        }
        self.record_count = record.sequence;
        self.last_record_digest = Some(record.record_digest.clone());
        Ok(())
    }

    fn advance(
        &mut self,
        record: &AuthorityWalRecordV1,
        required_previous: AuthorityWalPhase,
    ) -> Result<(), AuthorityWalError> {
        let previous = self
            .transactions
            .get(&record.transaction_id)
            .map(|replay| replay.phase);
        if previous != Some(required_previous) {
            return Err(AuthorityWalError::IllegalPhase {
                transaction_id: record.transaction_id.clone(),
                previous,
                attempted: record.phase,
            });
        }
        let transaction = &self
            .transactions
            .get(&record.transaction_id)
            .expect("phase equality proved transaction exists")
            .transaction;
        record.validate_against_transaction(transaction)?;
        self.transactions
            .get_mut(&record.transaction_id)
            .expect("phase equality proved transaction exists")
            .phase = record.phase;
        Ok(())
    }

    fn terminal_replay_for_prepare(
        &self,
        record: &AuthorityWalRecordV1,
        transaction: &AuthorityTransactionV1,
    ) -> Result<Option<AuthorityWalTerminalOutcome>, AuthorityWalError> {
        let scope = IdempotencyScope::from_transaction(transaction);
        if let Some(existing_id) = self.idempotency.get(&scope) {
            let existing = self
                .transactions
                .get(existing_id)
                .expect("idempotency index references an existing transaction");
            if let Some(terminal) = &existing.terminal {
                if existing.transaction.binding().intent_digest
                    == transaction.binding().intent_digest
                {
                    return Ok(Some(terminal.clone()));
                }
                return Err(AuthorityWalError::IdempotencyConflict {
                    idempotency_key: record.idempotency_key.clone(),
                });
            }
            return Err(AuthorityWalError::DuplicateIdempotency {
                idempotency_key: record.idempotency_key.clone(),
            });
        }
        if self.transactions.contains_key(&record.transaction_id) {
            return Err(AuthorityWalError::DuplicateTransaction {
                transaction_id: record.transaction_id.clone(),
            });
        }
        Ok(None)
    }
}

/// Isolated AuthorityWAL backend. Owning this value holds the process-wide and
/// cross-process writer lock; mutation requires `&mut self`. No runtime route
/// constructs this backend yet.
pub struct AuthorityWal {
    path: PathBuf,
    file: File,
    replay: ReplayIndex,
    recovery: AuthorityWalRecoveryReport,
    poisoned: bool,
    record_crypto: Arc<dyn AuthorityWalRecordCrypto>,
    protected_head_backend: Option<SharedProtectedJournalHeadBackendV1>,
    protected_head: Option<ProtectedJournalHeadSnapshotV1>,
    _writer_lock: LockGuard,
}

impl AuthorityWal {
    pub(crate) fn open(path: impl AsRef<Path>) -> Result<Self, AuthorityWalError> {
        Self::open_with_record_crypto(path, Arc::new(UnavailableAuthorityWalRecordCrypto))
    }

    pub fn open_software_test_not_production(
        path: impl AsRef<Path>,
    ) -> Result<Self, AuthorityWalError> {
        Self::open_with_record_crypto(
            path,
            Arc::new(
                SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(
                    b"m1nd-authority-wal-explicit-test-secret-v1",
                ),
            ),
        )
    }

    pub(crate) fn open_with_record_crypto(
        path: impl AsRef<Path>,
        record_crypto: Arc<dyn AuthorityWalRecordCrypto>,
    ) -> Result<Self, AuthorityWalError> {
        Self::open_internal(path, record_crypto, None)
    }

    pub(crate) fn open_with_record_crypto_and_protected_head(
        path: impl AsRef<Path>,
        record_crypto: Arc<dyn AuthorityWalRecordCrypto>,
        protected_head_backend: SharedProtectedJournalHeadBackendV1,
    ) -> Result<Self, AuthorityWalError> {
        Self::open_internal(path, record_crypto, Some(protected_head_backend))
    }

    fn open_internal(
        path: impl AsRef<Path>,
        record_crypto: Arc<dyn AuthorityWalRecordCrypto>,
        protected_head_backend: Option<SharedProtectedJournalHeadBackendV1>,
    ) -> Result<Self, AuthorityWalError> {
        let path = path.as_ref().to_path_buf();
        let parent = usable_parent(&path);
        refuse_symlink(parent)?;
        std::fs::create_dir_all(parent)?;
        refuse_symlink(parent)?;
        let lock_slug = format!(
            "{}.authority-wal-writer",
            path.file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_else(|| "authority.wal.jsonl".into())
        );
        let writer_lock = LockGuard::acquire_in(parent, &lock_slug)
            .map_err(|error| AuthorityWalError::WriterLock(error.to_string()))?;

        refuse_symlink(&path)?;
        let existed = path.exists();
        let mut file = open_journal_no_follow(&path)?;
        if !existed {
            file.sync_all()?;
            #[cfg(unix)]
            sync_parent_directory(parent)?;
        }
        let (replay, recovery, complete_len) = replay_file(&path, record_crypto.as_ref())?;
        let protected_head = protected_head_backend
            .as_ref()
            .map(|backend| {
                verify_or_initialize_protected_head(
                    backend,
                    AUTHORITY_WAL_HEAD_DOMAIN,
                    replay.record_count,
                    replay.last_record_digest.clone(),
                )
                .map_err(|detail| AuthorityWalError::ProtectedHead { detail })
            })
            .transpose()?;
        if recovery.truncated_tail_bytes != 0 {
            truncate_journal_tail(&file, &path, complete_len)?;
        }
        Ok(Self {
            path,
            file,
            replay,
            recovery,
            poisoned: false,
            record_crypto,
            protected_head_backend,
            protected_head,
            _writer_lock: writer_lock,
        })
    }

    pub fn crypto_assurance(&self) -> AuthorityWalCryptoAssurance {
        self.record_crypto.assurance()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn recovery_report(&self) -> &AuthorityWalRecoveryReport {
        &self.recovery
    }

    pub fn record_count(&self) -> u64 {
        self.replay.record_count
    }

    pub fn next_sequence(&self) -> u64 {
        self.replay.record_count + 1
    }

    pub fn last_record_digest(&self) -> Option<&str> {
        self.replay.last_record_digest.as_deref()
    }

    /// Returns terminal metadata for COMMIT or ABORT. It never returns a
    /// provisional payload.
    pub fn terminal_outcome(&self, transaction_id: &str) -> Option<&AuthorityWalTerminalOutcome> {
        self.replay
            .transactions
            .get(transaction_id)
            .and_then(|replay| replay.terminal.as_ref())
    }

    /// The only transaction visibility surface. PREPARE, PROVISIONAL, and
    /// ABORT all return `None`; only a durable COMMIT publishes the transaction.
    pub fn committed_transaction(&self, transaction_id: &str) -> Option<&AuthorityTransactionV1> {
        self.replay
            .transactions
            .get(transaction_id)
            .filter(|replay| replay.phase == AuthorityWalPhase::Commit)
            .map(|replay| &replay.transaction)
    }

    pub(crate) fn append(
        &mut self,
        mut record: AuthorityWalRecordV1,
    ) -> Result<AuthorityWalAppendOutcome, AuthorityWalError> {
        if self.poisoned {
            return Err(AuthorityWalError::Poisoned);
        }
        if let (Some(backend), Some(expected)) =
            (&self.protected_head_backend, &self.protected_head)
        {
            let observed = verify_or_initialize_protected_head(
                backend,
                AUTHORITY_WAL_HEAD_DOMAIN,
                self.replay.record_count,
                self.replay.last_record_digest.clone(),
            )
            .map_err(|detail| AuthorityWalError::ProtectedHead { detail })?;
            if &observed != expected {
                self.poisoned = true;
                return Err(AuthorityWalError::ProtectedHead {
                    detail: "protected WAL head changed outside owner serial".to_string(),
                });
            }
        }
        if self.record_crypto.assurance() == AuthorityWalCryptoAssurance::UnavailableFailClosed {
            return Err(AuthorityWalError::Crypto {
                operation: "sign",
                detail: "no explicit AuthorityWAL signer configured".to_string(),
            });
        }
        record.issuer = self.record_crypto.issuer().to_string();
        record.key_id = self.record_crypto.key_id().to_string();
        record.algorithm = self.record_crypto.algorithm().to_string();
        record.signature = m1nd_control::OpaqueSignature::new("pending-owner-signature");
        record
            .assign_chain_and_seal(self.next_sequence(), self.replay.last_record_digest.clone())
            .map_err(AuthorityWalContractError::from)?;
        let signing_message = record
            .canonical_signature_message()
            .map_err(AuthorityWalContractError::from)?;
        record.signature =
            m1nd_control::OpaqueSignature::new(self.record_crypto.sign(&signing_message).map_err(
                |detail| AuthorityWalError::Crypto {
                    operation: "sign",
                    detail,
                },
            )?);
        verify_record_crypto(&record, self.record_crypto.as_ref())?;
        record.validate()?;

        if let AuthorityWalPayloadV1::Prepare(payload) = &record.payload {
            record.validate_against_transaction(&payload.transaction)?;
            if let Some(terminal) = self
                .replay
                .terminal_replay_for_prepare(&record, &payload.transaction)?
            {
                return Ok(AuthorityWalAppendOutcome::TerminalReplay(terminal));
            }
        }

        let mut next_replay = self.replay.clone();
        next_replay.apply_record(&record)?;
        let verified_commit_witness = next_replay
            .transactions
            .get(&record.transaction_id)
            .and_then(|replay| replay.terminal.as_ref())
            .and_then(AuthorityWalTerminalOutcome::verified_commit_witness);

        let mut bytes = serde_json::to_vec(&record)?;
        bytes.push(b'\n');
        if let Err(error) = self
            .file
            .write_all(&bytes)
            .and_then(|()| self.file.flush())
            .and_then(|()| self.file.sync_all())
        {
            self.poisoned = true;
            return Err(AuthorityWalError::Io(error));
        }

        if let (Some(backend), Some(expected)) =
            (&self.protected_head_backend, &self.protected_head)
        {
            match advance_protected_head(
                backend,
                AUTHORITY_WAL_HEAD_DOMAIN,
                expected,
                record.sequence,
                record.record_digest.clone(),
            ) {
                Ok(next) => self.protected_head = Some(next),
                Err(detail) => {
                    self.poisoned = true;
                    return Err(AuthorityWalError::ProtectedHead { detail });
                }
            }
        }

        self.replay = next_replay;
        self.recovery.record_count = self.replay.record_count;
        Ok(AuthorityWalAppendOutcome::Appended {
            sequence: record.sequence,
            phase: record.phase,
            record_digest: record.record_digest,
            verified_commit_witness,
        })
    }
}

fn replay_file(
    path: &Path,
    record_crypto: &dyn AuthorityWalRecordCrypto,
) -> Result<(ReplayIndex, AuthorityWalRecoveryReport, u64), AuthorityWalError> {
    let bytes = std::fs::read(path)?;
    let complete_len = if bytes.ends_with(b"\n") {
        bytes.len()
    } else {
        bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1)
    };
    let truncated_tail_bytes = (bytes.len() - complete_len) as u64;

    let mut replay = ReplayIndex::default();
    let mut offset = 0_u64;
    for frame in bytes[..complete_len].split_inclusive(|byte| *byte == b'\n') {
        let line = &frame[..frame.len() - 1];
        if line.is_empty() {
            return Err(AuthorityWalError::Corruption {
                offset,
                detail: "empty newline-framed record".to_string(),
            });
        }
        let record: AuthorityWalRecordV1 =
            serde_json::from_slice(line).map_err(|error| AuthorityWalError::Corruption {
                offset,
                detail: format!("invalid JSON record: {error}"),
            })?;
        let expected_sequence = replay.record_count + 1;
        if record.sequence != expected_sequence {
            return Err(AuthorityWalError::Corruption {
                offset,
                detail: AuthorityWalError::SequenceMismatch {
                    expected: expected_sequence,
                    observed: record.sequence,
                }
                .to_string(),
            });
        }
        if record.previous_record_digest != replay.last_record_digest {
            return Err(AuthorityWalError::Corruption {
                offset,
                detail: AuthorityWalError::HashChainMismatch {
                    sequence: record.sequence,
                }
                .to_string(),
            });
        }
        record
            .validate()
            .map_err(|error| AuthorityWalError::Corruption {
                offset,
                detail: error.to_string(),
            })?;
        verify_record_crypto(&record, record_crypto).map_err(|error| {
            AuthorityWalError::Corruption {
                offset,
                detail: error.to_string(),
            }
        })?;
        replay
            .apply_record(&record)
            .map_err(|error| AuthorityWalError::Corruption {
                offset,
                detail: error.to_string(),
            })?;
        offset += frame.len() as u64;
    }
    Ok((
        replay,
        AuthorityWalRecoveryReport {
            record_count: offset_to_record_count(&bytes[..complete_len]),
            recovered_torn_tail: truncated_tail_bytes != 0,
            truncated_tail_bytes,
        },
        complete_len as u64,
    ))
}

fn verify_record_crypto(
    record: &AuthorityWalRecordV1,
    record_crypto: &dyn AuthorityWalRecordCrypto,
) -> Result<(), AuthorityWalError> {
    if record_crypto.assurance() == AuthorityWalCryptoAssurance::UnavailableFailClosed {
        return Err(AuthorityWalError::Crypto {
            operation: "verify",
            detail: "no explicit AuthorityWAL verifier configured".to_string(),
        });
    }
    if record.issuer != record_crypto.issuer()
        || record.key_id != record_crypto.key_id()
        || record.algorithm != record_crypto.algorithm()
    {
        return Err(AuthorityWalError::Crypto {
            operation: "verify",
            detail: "record signer metadata does not match the pinned WAL verifier".to_string(),
        });
    }
    let message = record
        .canonical_signature_message()
        .map_err(AuthorityWalContractError::from)?;
    record_crypto
        .verify(&message, record.signature.as_str())
        .map_err(|detail| AuthorityWalError::Crypto {
            operation: "verify",
            detail,
        })
}

fn offset_to_record_count(bytes: &[u8]) -> u64 {
    bytes.iter().filter(|byte| **byte == b'\n').count() as u64
}

fn usable_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn refuse_symlink(path: &Path) -> Result<(), AuthorityWalError> {
    if path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata_is_link_or_reparse(&metadata))
    {
        return Err(AuthorityWalError::ProtectedHead {
            detail: format!("symlink refused for AuthorityWAL path {}", path.display()),
        });
    }
    Ok(())
}

fn metadata_is_link_or_reparse(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        crate::windows_durable_fs::is_reparse_point(metadata)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(unix)]
fn open_journal_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

#[cfg(windows)]
fn open_journal_no_follow(path: &Path) -> io::Result<File> {
    crate::windows_durable_fs::open_read_append_create_no_follow(path)
}

#[cfg(all(not(unix), not(windows)))]
fn open_journal_no_follow(_path: &Path) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "AuthorityWAL no-follow durable open is not proven on this platform",
    ))
}

#[cfg(unix)]
fn truncate_journal_tail(file: &File, _path: &Path, complete_len: u64) -> io::Result<()> {
    file.set_len(complete_len)?;
    file.sync_all()
}

#[cfg(windows)]
fn truncate_journal_tail(_file: &File, path: &Path, complete_len: u64) -> io::Result<()> {
    // The journal handle is append-only on Windows (no `FILE_WRITE_DATA`), so it
    // cannot `SetEndOfFile`; truncate the torn tail through a dedicated write
    // handle instead.
    crate::windows_durable_fs::truncate_no_follow(path, complete_len)
}

#[cfg(all(not(unix), not(windows)))]
fn truncate_journal_tail(_file: &File, _path: &Path, _complete_len: u64) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "AuthorityWAL torn-tail truncation is not proven on this platform",
    ))
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> io::Result<()> {
    File::open(parent)?.sync_all()
}

#[cfg(test)]
mod tests {
    use m1nd_control::{
        ActiveMode, AuthorityTransactionBindingV1, AuthorityVariant, AuthorityWalAbortV1,
        AuthorityWalCommitV1, AuthorityWalPrepareV1, AuthorityWalProvisionalV1, CapabilityKind,
        OpaqueSignature, PositiveAuthorityTransactionV1, POSITIVE_AUTHORITY_TRANSACTION_SCHEMA,
    };

    use super::*;

    fn hash(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn transaction(
        transaction_id: &str,
        idempotency_key: &str,
        intent: char,
    ) -> AuthorityTransactionV1 {
        let mut transaction =
            AuthorityTransactionV1::PositiveAuthority(PositiveAuthorityTransactionV1 {
                schema: POSITIVE_AUTHORITY_TRANSACTION_SCHEMA.to_string(),
                binding: AuthorityTransactionBindingV1 {
                    transaction_id: transaction_id.to_string(),
                    organism_id: "organism-1".to_string(),
                    brain_id: "brain-1".to_string(),
                    subject_id: "subject-1".to_string(),
                    action_id: "land".to_string(),
                    idempotency_key: idempotency_key.to_string(),
                    intent_core_ref: format!("intent:{intent}"),
                    intent_digest: hash(intent),
                    intent_canonicalization_version: "m1nd-canonical-json-v1".to_string(),
                    capability_id: format!("capability-{transaction_id}"),
                    capability_kind: CapabilityKind::Human,
                    nonce: format!("nonce-{transaction_id}"),
                    expected_head_id: Some("head-1".to_string()),
                    expected_active_mode: ActiveMode::HumanGated,
                    expected_activation_receipt_id: None,
                    expected_constitution_epoch: 2,
                    expected_autonomy_epoch: 7,
                    expected_store_epoch: 11,
                    sentinel_verdict_digest: None,
                    authorization_snapshot_digest: hash('b'),
                    issued_at: 1_000,
                    expires_at: 3_000,
                },
                authority_decision_digest: hash('c'),
                identity_role_binding_digest: hash('d'),
                required_authority_variant: AuthorityVariant::Human,
                action_policy_registry_digest: hash('e'),
                classifier_decision_digest: hash('f'),
                expected_pending_red_set_digest: hash('1'),
                expected_red_latch_epoch: 3,
                expected_store_version: 19,
                expected_boundary_version: 5,
                expected_contract_version: 8,
                action_payload_digest: hash('2'),
                issuer: "owner-1".to_string(),
                key_id: "owner-key-1".to_string(),
                algorithm: "opaque-test-algorithm".to_string(),
                transaction_digest: hash('0'),
                signature: OpaqueSignature::new("opaque-transaction-signature"),
            });
        transaction.seal().unwrap();
        transaction
    }

    fn draft(
        transaction: &AuthorityTransactionV1,
        phase: AuthorityWalPhase,
    ) -> AuthorityWalRecordV1 {
        let payload = match phase {
            AuthorityWalPhase::Prepare => {
                AuthorityWalPayloadV1::Prepare(Box::new(AuthorityWalPrepareV1 {
                    transaction: transaction.clone(),
                }))
            }
            AuthorityWalPhase::Provisional => {
                AuthorityWalPayloadV1::Provisional(AuthorityWalProvisionalV1 {
                    provisional_effects_digest: hash('a'),
                })
            }
            AuthorityWalPhase::Commit => AuthorityWalPayloadV1::Commit(AuthorityWalCommitV1 {
                committed_at: 1_500,
                protected_time_evidence_digest: hash('b'),
                authorization_snapshot_digest: transaction
                    .binding()
                    .authorization_snapshot_digest
                    .clone(),
                terminal_outcome_digest: hash('c'),
            }),
            AuthorityWalPhase::Abort => AuthorityWalPayloadV1::Abort(AuthorityWalAbortV1 {
                aborted_at: 1_500,
                reason_digest: hash('d'),
                terminal_outcome_digest: hash('e'),
            }),
        };
        AuthorityWalRecordV1::draft(
            transaction,
            payload,
            1_600,
            "owner-1",
            "owner-key-1",
            "opaque-test-algorithm",
            OpaqueSignature::new("opaque-record-signature"),
        )
    }

    fn append_phase(
        wal: &mut AuthorityWal,
        transaction: &AuthorityTransactionV1,
        phase: AuthorityWalPhase,
    ) -> AuthorityWalAppendOutcome {
        wal.append(draft(transaction, phase)).unwrap()
    }

    fn wal_path(directory: &tempfile::TempDir) -> PathBuf {
        directory.path().join("authority.wal.jsonl")
    }

    #[test]
    fn legal_prepare_provisional_commit_is_durable_and_only_commit_is_visible() {
        let directory = tempfile::tempdir().unwrap();
        let path = wal_path(&directory);
        let transaction = transaction("tx-1", "idem-1", 'a');

        let mut wal = AuthorityWal::open_software_test_not_production(&path).unwrap();
        assert_eq!(wal.record_count(), 0);
        assert!(!wal.recovery_report().recovered_torn_tail);

        append_phase(&mut wal, &transaction, AuthorityWalPhase::Prepare);
        assert!(wal.committed_transaction("tx-1").is_none());
        assert!(wal.terminal_outcome("tx-1").is_none());

        append_phase(&mut wal, &transaction, AuthorityWalPhase::Provisional);
        assert!(wal.committed_transaction("tx-1").is_none());
        assert!(wal.terminal_outcome("tx-1").is_none());

        append_phase(&mut wal, &transaction, AuthorityWalPhase::Commit);
        assert_eq!(wal.record_count(), 3);
        assert_eq!(
            wal.terminal_outcome("tx-1").unwrap().phase,
            AuthorityWalPhase::Commit
        );
        assert_eq!(wal.committed_transaction("tx-1"), Some(&transaction));
        drop(wal);

        let reopened = AuthorityWal::open_software_test_not_production(&path).unwrap();
        assert_eq!(reopened.record_count(), 3);
        assert_eq!(reopened.committed_transaction("tx-1"), Some(&transaction));
        assert!(!reopened.recovery_report().recovered_torn_tail);
    }

    #[test]
    fn protected_head_refuses_replacement_with_valid_older_wal_prefix() {
        let directory = tempfile::tempdir().unwrap();
        let path = wal_path(&directory);
        let transaction = transaction("tx-protected-rollback", "idem-protected-rollback", 'a');
        let protected =
            crate::protected_journal_head::SoftwareTestProtectedJournalHeadBackendV1::new();
        let shared = protected.clone().shared();
        let crypto: Arc<dyn AuthorityWalRecordCrypto> = Arc::new(
            SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(
                b"protected-wal-test-only",
            ),
        );
        let mut wal = AuthorityWal::open_with_record_crypto_and_protected_head(
            &path,
            Arc::clone(&crypto),
            Arc::clone(&shared),
        )
        .unwrap();
        append_phase(&mut wal, &transaction, AuthorityWalPhase::Prepare);
        assert_eq!(
            protected
                .snapshot(crate::protected_journal_head::AUTHORITY_WAL_HEAD_DOMAIN)
                .unwrap()
                .record_sequence,
            1
        );
        drop(wal);

        std::fs::write(&path, []).unwrap();
        let reopened =
            AuthorityWal::open_with_record_crypto_and_protected_head(&path, crypto, shared);
        assert!(matches!(
            reopened,
            Err(AuthorityWalError::ProtectedHead { .. })
        ));
    }

    #[test]
    fn abort_is_terminal_but_never_publishes_provisional_effects() {
        let directory = tempfile::tempdir().unwrap();
        let transaction = transaction("tx-abort", "idem-abort", 'a');
        let mut wal =
            AuthorityWal::open_software_test_not_production(wal_path(&directory)).unwrap();
        append_phase(&mut wal, &transaction, AuthorityWalPhase::Prepare);
        append_phase(&mut wal, &transaction, AuthorityWalPhase::Provisional);
        append_phase(&mut wal, &transaction, AuthorityWalPhase::Abort);
        assert_eq!(
            wal.terminal_outcome("tx-abort").unwrap().phase,
            AuthorityWalPhase::Abort
        );
        assert!(wal.committed_transaction("tx-abort").is_none());
    }

    #[test]
    fn illegal_phase_edges_fail_before_any_append() {
        let directory = tempfile::tempdir().unwrap();
        let transaction = transaction("tx-1", "idem-1", 'a');
        let mut wal =
            AuthorityWal::open_software_test_not_production(wal_path(&directory)).unwrap();

        assert!(matches!(
            wal.append(draft(&transaction, AuthorityWalPhase::Provisional)),
            Err(AuthorityWalError::IllegalPhase {
                previous: None,
                attempted: AuthorityWalPhase::Provisional,
                ..
            })
        ));
        assert_eq!(wal.record_count(), 0);

        append_phase(&mut wal, &transaction, AuthorityWalPhase::Prepare);
        assert!(matches!(
            wal.append(draft(&transaction, AuthorityWalPhase::Commit)),
            Err(AuthorityWalError::IllegalPhase {
                previous: Some(AuthorityWalPhase::Prepare),
                attempted: AuthorityWalPhase::Commit,
                ..
            })
        ));
        assert_eq!(wal.record_count(), 1);

        append_phase(&mut wal, &transaction, AuthorityWalPhase::Abort);
        assert!(matches!(
            wal.append(draft(&transaction, AuthorityWalPhase::Provisional)),
            Err(AuthorityWalError::IllegalPhase {
                previous: Some(AuthorityWalPhase::Abort),
                ..
            })
        ));
        assert_eq!(wal.record_count(), 2);
    }

    #[test]
    fn duplicate_transaction_and_inflight_idempotency_are_refused() {
        let directory = tempfile::tempdir().unwrap();
        let original = transaction("tx-1", "idem-1", 'a');
        let mut wal =
            AuthorityWal::open_software_test_not_production(wal_path(&directory)).unwrap();
        append_phase(&mut wal, &original, AuthorityWalPhase::Prepare);

        let duplicate_transaction = transaction("tx-1", "idem-other", 'a');
        assert!(matches!(
            wal.append(draft(&duplicate_transaction, AuthorityWalPhase::Prepare)),
            Err(AuthorityWalError::DuplicateTransaction { .. })
        ));

        let duplicate_idempotency = transaction("tx-2", "idem-1", 'a');
        assert!(matches!(
            wal.append(draft(&duplicate_idempotency, AuthorityWalPhase::Prepare)),
            Err(AuthorityWalError::DuplicateIdempotency { .. })
        ));
        assert_eq!(wal.record_count(), 1);
    }

    #[test]
    fn terminal_idempotency_replay_returns_prior_outcome_without_append() {
        let directory = tempfile::tempdir().unwrap();
        let original = transaction("tx-1", "idem-1", 'a');
        let mut wal =
            AuthorityWal::open_software_test_not_production(wal_path(&directory)).unwrap();
        append_phase(&mut wal, &original, AuthorityWalPhase::Prepare);
        append_phase(&mut wal, &original, AuthorityWalPhase::Provisional);
        append_phase(&mut wal, &original, AuthorityWalPhase::Commit);
        let record_count = wal.record_count();

        let retry = transaction("tx-retry", "idem-1", 'a');
        let replay = wal
            .append(draft(&retry, AuthorityWalPhase::Prepare))
            .unwrap();
        assert!(matches!(
            replay,
            AuthorityWalAppendOutcome::TerminalReplay(AuthorityWalTerminalOutcome {
                phase: AuthorityWalPhase::Commit,
                ref transaction_id,
                ..
            }) if transaction_id == "tx-1"
        ));
        assert_eq!(wal.record_count(), record_count);
        assert!(wal.committed_transaction("tx-retry").is_none());

        let conflict = transaction("tx-conflict", "idem-1", 'f');
        assert!(matches!(
            wal.append(draft(&conflict, AuthorityWalPhase::Prepare)),
            Err(AuthorityWalError::IdempotencyConflict { .. })
        ));
        assert_eq!(wal.record_count(), record_count);
    }

    #[test]
    fn every_partial_byte_boundary_of_the_final_record_recovers_only_that_tail() {
        let source_directory = tempfile::tempdir().unwrap();
        let source_path = wal_path(&source_directory);
        let transaction = transaction("tx-1", "idem-1", 'a');
        let mut source = AuthorityWal::open_software_test_not_production(&source_path).unwrap();
        append_phase(&mut source, &transaction, AuthorityWalPhase::Prepare);
        append_phase(&mut source, &transaction, AuthorityWalPhase::Provisional);
        append_phase(&mut source, &transaction, AuthorityWalPhase::Commit);
        drop(source);

        let complete = std::fs::read(&source_path).unwrap();
        let newline_positions: Vec<usize> = complete
            .iter()
            .enumerate()
            .filter_map(|(index, byte)| (*byte == b'\n').then_some(index))
            .collect();
        assert_eq!(newline_positions.len(), 3);
        let final_record_start = newline_positions[1] + 1;

        for cut in (final_record_start + 1)..complete.len() {
            let case_directory = tempfile::tempdir().unwrap();
            let case_path = wal_path(&case_directory);
            std::fs::write(&case_path, &complete[..cut]).unwrap();
            let recovered = AuthorityWal::open_software_test_not_production(&case_path).unwrap();
            assert_eq!(recovered.record_count(), 2, "cut at byte {cut}");
            assert!(
                recovered.recovery_report().recovered_torn_tail,
                "cut at byte {cut}"
            );
            assert_eq!(
                recovered.recovery_report().truncated_tail_bytes,
                (cut - final_record_start) as u64,
                "cut at byte {cut}"
            );
            assert!(recovered.committed_transaction("tx-1").is_none());
            assert_eq!(
                std::fs::metadata(&case_path).unwrap().len(),
                final_record_start as u64
            );
        }
    }

    #[test]
    fn newline_terminated_final_corruption_is_not_misclassified_as_torn_tail() {
        let directory = tempfile::tempdir().unwrap();
        let path = wal_path(&directory);
        let transaction = transaction("tx-1", "idem-1", 'a');
        let mut wal = AuthorityWal::open_software_test_not_production(&path).unwrap();
        append_phase(&mut wal, &transaction, AuthorityWalPhase::Prepare);
        append_phase(&mut wal, &transaction, AuthorityWalPhase::Provisional);
        drop(wal);

        let mut bytes = std::fs::read(&path).unwrap();
        let final_start = bytes[..bytes.len() - 1]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        bytes[final_start] = b'[';
        std::fs::write(&path, &bytes).unwrap();
        assert!(matches!(
            AuthorityWal::open_software_test_not_production(&path),
            Err(AuthorityWalError::Corruption { .. })
        ));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), bytes.len() as u64);
    }

    #[test]
    fn internal_corruption_and_swapped_payload_fail_closed() {
        let source_directory = tempfile::tempdir().unwrap();
        let source_path = wal_path(&source_directory);
        let transaction = transaction("tx-1", "idem-1", 'a');
        let mut wal = AuthorityWal::open_software_test_not_production(&source_path).unwrap();
        append_phase(&mut wal, &transaction, AuthorityWalPhase::Prepare);
        append_phase(&mut wal, &transaction, AuthorityWalPhase::Provisional);
        append_phase(&mut wal, &transaction, AuthorityWalPhase::Commit);
        drop(wal);
        let original = std::fs::read(&source_path).unwrap();

        let internal_directory = tempfile::tempdir().unwrap();
        let internal_path = wal_path(&internal_directory);
        let mut internal = original.clone();
        let tx_offset = internal
            .windows(b"tx-1".len())
            .position(|window| window == b"tx-1")
            .unwrap();
        internal[tx_offset] = b'u';
        internal.truncate(internal.len() - 5);
        let corrupt_length = internal.len() as u64;
        std::fs::write(&internal_path, &internal).unwrap();
        assert!(matches!(
            AuthorityWal::open_software_test_not_production(&internal_path),
            Err(AuthorityWalError::Corruption { .. })
        ));
        assert_eq!(
            std::fs::metadata(&internal_path).unwrap().len(),
            corrupt_length,
            "an internal corruption must fail closed before tail repair mutates the file"
        );

        let swapped_directory = tempfile::tempdir().unwrap();
        let swapped_path = wal_path(&swapped_directory);
        let mut swapped = original;
        let provisional_marker = format!("\"provisional_effects_digest\":\"{}\"", hash('a'));
        let marker_offset = swapped
            .windows(provisional_marker.len())
            .position(|window| window == provisional_marker.as_bytes())
            .unwrap();
        let digest_offset = marker_offset + "\"provisional_effects_digest\":\"".len();
        swapped[digest_offset] = b'b';
        std::fs::write(&swapped_path, &swapped).unwrap();
        assert!(matches!(
            AuthorityWal::open_software_test_not_production(&swapped_path),
            Err(AuthorityWalError::Corruption { .. })
        ));
    }

    #[test]
    fn persisted_hash_chain_and_sequence_are_replayed_strictly() {
        let directory = tempfile::tempdir().unwrap();
        let path = wal_path(&directory);
        let transaction = transaction("tx-1", "idem-1", 'a');
        let mut wal = AuthorityWal::open_software_test_not_production(&path).unwrap();
        let first = append_phase(&mut wal, &transaction, AuthorityWalPhase::Prepare);
        let first_digest = match first {
            AuthorityWalAppendOutcome::Appended { record_digest, .. } => record_digest,
            AuthorityWalAppendOutcome::TerminalReplay(_) => unreachable!(),
        };
        let second = append_phase(&mut wal, &transaction, AuthorityWalPhase::Provisional);
        assert!(matches!(
            second,
            AuthorityWalAppendOutcome::Appended { sequence: 2, .. }
        ));
        assert_ne!(wal.last_record_digest(), Some(first_digest.as_str()));
        drop(wal);

        let reopened = AuthorityWal::open_software_test_not_production(&path).unwrap();
        assert_eq!(reopened.record_count(), 2);
        assert_eq!(reopened.next_sequence(), 3);
    }
}
