//! Content-addressed, old-or-new checkpoint storage for one brain.
//!
//! Checkpoints are assembled in a staging directory beneath the checkpoint
//! parent, fsynced, and atomically renamed into their immutable digest name.
//! Only then is `CURRENT` atomically replaced and its parent fsynced. A complete
//! directory that was never selected by `CURRENT` remains an orphan candidate;
//! recovery never promotes it by mtime, generation, or guesswork.
//!
//! External authority roots are deliberately revalidated through an injected
//! verifier. This module verifies the verifier receipt bindings, but does not
//! pretend to own AuthorityWAL, IntentCoreStore, Sentinel, or AutonomyEpoch.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use m1nd_control::digest_canonical;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CHECKPOINT_MANIFEST_SCHEMA: &str = "m1nd-checkpoint-manifest-v1";
pub const CHECKPOINT_FILE_SCHEMA: &str = "m1nd-checkpoint-file-v1";
pub const CHECKPOINT_CURRENT_SCHEMA: &str = "m1nd-checkpoint-current-v1";
pub const CHECKPOINT_ACK_SCHEMA: &str = "m1nd-checkpoint-ack-v1";
pub const CHECKPOINT_EVICTION_PERMIT_SCHEMA: &str = "m1nd-checkpoint-eviction-permit-v1";
pub const CHECKPOINT_AUTHORITY_RECEIPT_SCHEMA: &str =
    "m1nd-checkpoint-authority-validation-receipt-v1";
pub const CHECKPOINT_FALLBACK_RECEIPT_SCHEMA: &str = "m1nd-checkpoint-fallback-receipt-v1";
pub const CHECKPOINT_GC_RECEIPT_SCHEMA: &str = "m1nd-checkpoint-gc-receipt-v1";

pub const GRAPH_SNAPSHOT_LOGICAL_NAME: &str = "graph_snapshot";
pub const INGEST_ROOTS_LOGICAL_NAME: &str = "ingest_roots";

const CHECKPOINT_ID_DOMAIN: &str = "m1nd-checkpoint-manifest-v1";
const CURRENT_POINTER_DIGEST_DOMAIN: &str = "m1nd-checkpoint-current-v1";
const EXTERNAL_AUTHORITY_REFS_DIGEST_DOMAIN: &str = "m1nd-checkpoint-authority-refs-v1";
const AUTHORITY_RECEIPT_DIGEST_DOMAIN: &str = "m1nd-checkpoint-authority-validation-receipt-v1";
const FALLBACK_RECEIPT_DIGEST_DOMAIN: &str = "m1nd-checkpoint-fallback-receipt-v1";
const GC_RECEIPT_DIGEST_DOMAIN: &str = "m1nd-checkpoint-gc-receipt-v1";

const CHECKPOINTS_DIRECTORY: &str = "checkpoints";
const BLOBS_DIRECTORY: &str = "files";
const MANIFEST_FILE: &str = "manifest.json";
const CURRENT_FILE: &str = "CURRENT";
const WRITER_LOCK_FILE: &str = "WRITER.lock";
const MAX_POINTER_BYTES: u64 = 64 * 1024;
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

static TEMP_NONCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointExternalAuthorityRefsV1 {
    pub system_block_store_version: u64,
    pub mission_heads_index_digest: String,
    pub authority_wal_root_digest: String,
    pub intent_core_store_root_digest: String,
    pub sentinel_outbox_watermark_digest: String,
    pub autonomy_epoch_record_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointFileV1 {
    pub schema: String,
    pub logical_name: String,
    pub relative_path: String,
    pub schema_id: String,
    pub schema_version: String,
    pub content_digest: String,
    pub byte_len: u64,
    pub blob_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointManifestV1 {
    pub schema: String,
    pub checkpoint_id: String,
    pub brain_id: String,
    pub epoch: u64,
    pub generation: u64,
    pub revision: u64,
    pub schema_versions: BTreeMap<String, String>,
    pub file_inventory: Vec<CheckpointFileV1>,
    pub graph_snapshot_digest: String,
    pub sidecar_digests: BTreeMap<String, String>,
    pub ingest_roots_digest: String,
    pub external_authority_refs: CheckpointExternalAuthorityRefsV1,
    pub created_at_unix_ms: u64,
    pub previous_checkpoint_id: Option<String>,
}

#[derive(Serialize)]
struct CheckpointManifestCore<'a> {
    schema: &'a str,
    brain_id: &'a str,
    epoch: u64,
    generation: u64,
    revision: u64,
    schema_versions: &'a BTreeMap<String, String>,
    file_inventory: &'a [CheckpointFileV1],
    graph_snapshot_digest: &'a str,
    sidecar_digests: &'a BTreeMap<String, String>,
    ingest_roots_digest: &'a str,
    external_authority_refs: &'a CheckpointExternalAuthorityRefsV1,
    created_at_unix_ms: u64,
    previous_checkpoint_id: &'a Option<String>,
}

impl CheckpointManifestV1 {
    pub fn compute_checkpoint_id(&self) -> Result<String, CheckpointError> {
        let core = CheckpointManifestCore {
            schema: &self.schema,
            brain_id: &self.brain_id,
            epoch: self.epoch,
            generation: self.generation,
            revision: self.revision,
            schema_versions: &self.schema_versions,
            file_inventory: &self.file_inventory,
            graph_snapshot_digest: &self.graph_snapshot_digest,
            sidecar_digests: &self.sidecar_digests,
            ingest_roots_digest: &self.ingest_roots_digest,
            external_authority_refs: &self.external_authority_refs,
            created_at_unix_ms: self.created_at_unix_ms,
            previous_checkpoint_id: &self.previous_checkpoint_id,
        };
        let value = serde_json::to_value(core)?;
        digest_canonical(CHECKPOINT_ID_DOMAIN, &value)
            .map_err(|error| CheckpointError::Canonical(error.to_string()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointFileInputV1 {
    pub logical_name: String,
    pub relative_path: String,
    pub schema_id: String,
    pub schema_version: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointCreateV1 {
    pub brain_id: String,
    pub epoch: u64,
    pub generation: u64,
    pub revision: u64,
    pub schema_versions: BTreeMap<String, String>,
    pub files: Vec<CheckpointFileInputV1>,
    pub external_authority_refs: CheckpointExternalAuthorityRefsV1,
    pub created_at_unix_ms: u64,
    pub expected_current_checkpoint_id: Option<String>,
}

/// Deterministically build the manifest that `create_checkpoint` will attempt
/// to publish without touching the checkpoint store.  Actor callers use this
/// identity to distinguish a pre-CURRENT failure from a committed-but-
/// unconfirmed result after an I/O or authority readback error.
pub(crate) fn preview_checkpoint_manifest(
    input: &CheckpointCreateV1,
) -> Result<CheckpointManifestV1, CheckpointError> {
    build_checkpoint(input.clone()).map(|built| built.manifest)
}

/// Read a candidate working file with the same no-follow primitive used by the
/// immutable checkpoint store. Callers decide whether `NotFound` is an allowed
/// absence before invoking this function; every other path/type/I/O failure is
/// fatal to checkpoint completeness.
pub(crate) fn read_regular_checkpoint_input(path: &Path) -> Result<Vec<u8>, CheckpointError> {
    read_regular_file_no_follow(path, None)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointCurrentV1 {
    pub schema: String,
    pub pointer_revision: u64,
    pub current_checkpoint_id: String,
    pub fallback_checkpoint_id: Option<String>,
    pub previous_pointer_digest: Option<String>,
    pub pointer_digest: String,
}

#[derive(Serialize)]
struct CheckpointCurrentCore<'a> {
    schema: &'a str,
    pointer_revision: u64,
    current_checkpoint_id: &'a str,
    fallback_checkpoint_id: &'a Option<String>,
    previous_pointer_digest: &'a Option<String>,
}

impl CheckpointCurrentV1 {
    fn compute_digest(&self) -> Result<String, CheckpointError> {
        let core = CheckpointCurrentCore {
            schema: &self.schema,
            pointer_revision: self.pointer_revision,
            current_checkpoint_id: &self.current_checkpoint_id,
            fallback_checkpoint_id: &self.fallback_checkpoint_id,
            previous_pointer_digest: &self.previous_pointer_digest,
        };
        let value = serde_json::to_value(core)?;
        digest_canonical(CURRENT_POINTER_DIGEST_DOMAIN, &value)
            .map_err(|error| CheckpointError::Canonical(error.to_string()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointAuthorityValidationReceiptV1 {
    pub schema: String,
    pub validator_id: String,
    pub checkpoint_id: String,
    pub external_authority_refs_digest: String,
    pub protected_root_digest: String,
    pub verified_at_unix_ms: u64,
    pub receipt_digest: String,
}

#[derive(Serialize)]
struct AuthorityReceiptCore<'a> {
    schema: &'a str,
    validator_id: &'a str,
    checkpoint_id: &'a str,
    external_authority_refs_digest: &'a str,
    protected_root_digest: &'a str,
    verified_at_unix_ms: u64,
}

impl CheckpointAuthorityValidationReceiptV1 {
    pub fn verified(
        validator_id: impl Into<String>,
        checkpoint_id: impl Into<String>,
        external_authority_refs_digest: impl Into<String>,
        protected_root_digest: impl Into<String>,
        verified_at_unix_ms: u64,
    ) -> Result<Self, CheckpointError> {
        let mut receipt = Self {
            schema: CHECKPOINT_AUTHORITY_RECEIPT_SCHEMA.to_string(),
            validator_id: validator_id.into(),
            checkpoint_id: checkpoint_id.into(),
            external_authority_refs_digest: external_authority_refs_digest.into(),
            protected_root_digest: protected_root_digest.into(),
            verified_at_unix_ms,
            receipt_digest: String::new(),
        };
        receipt.receipt_digest = receipt.compute_digest()?;
        Ok(receipt)
    }

    fn compute_digest(&self) -> Result<String, CheckpointError> {
        let core = AuthorityReceiptCore {
            schema: &self.schema,
            validator_id: &self.validator_id,
            checkpoint_id: &self.checkpoint_id,
            external_authority_refs_digest: &self.external_authority_refs_digest,
            protected_root_digest: &self.protected_root_digest,
            verified_at_unix_ms: self.verified_at_unix_ms,
        };
        let value = serde_json::to_value(core)?;
        digest_canonical(AUTHORITY_RECEIPT_DIGEST_DOMAIN, &value)
            .map_err(|error| CheckpointError::Canonical(error.to_string()))
    }
}

pub trait CheckpointAuthorityValidator: Send + Sync {
    fn validate(
        &self,
        manifest: &CheckpointManifestV1,
        external_authority_refs_digest: &str,
    ) -> Result<CheckpointAuthorityValidationReceiptV1, String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckpointLoadDisposition {
    ExactCurrent,
    DegradedFallback,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointFallbackReceiptV1 {
    pub schema: String,
    pub requested_checkpoint_id: String,
    pub selected_checkpoint_id: String,
    pub current_pointer_digest: String,
    pub failure_code: String,
    pub failure_detail_digest: String,
    pub issued_at_unix_ms: u64,
    pub receipt_digest: String,
}

#[derive(Serialize)]
struct FallbackReceiptCore<'a> {
    schema: &'a str,
    requested_checkpoint_id: &'a str,
    selected_checkpoint_id: &'a str,
    current_pointer_digest: &'a str,
    failure_code: &'a str,
    failure_detail_digest: &'a str,
    issued_at_unix_ms: u64,
}

impl CheckpointFallbackReceiptV1 {
    fn new(
        requested_checkpoint_id: String,
        selected_checkpoint_id: String,
        current_pointer_digest: String,
        failure_code: String,
        failure_detail: &str,
        issued_at_unix_ms: u64,
    ) -> Result<Self, CheckpointError> {
        if issued_at_unix_ms == 0 {
            return Err(CheckpointError::refused(
                "checkpoint_fallback_receipt_time_missing",
                "issued_at_unix_ms must be non-zero",
            ));
        }
        let mut receipt = Self {
            schema: CHECKPOINT_FALLBACK_RECEIPT_SCHEMA.to_string(),
            requested_checkpoint_id,
            selected_checkpoint_id,
            current_pointer_digest,
            failure_code,
            failure_detail_digest: sha256_bytes(failure_detail.as_bytes()),
            issued_at_unix_ms,
            receipt_digest: String::new(),
        };
        let core = FallbackReceiptCore {
            schema: &receipt.schema,
            requested_checkpoint_id: &receipt.requested_checkpoint_id,
            selected_checkpoint_id: &receipt.selected_checkpoint_id,
            current_pointer_digest: &receipt.current_pointer_digest,
            failure_code: &receipt.failure_code,
            failure_detail_digest: &receipt.failure_detail_digest,
            issued_at_unix_ms: receipt.issued_at_unix_ms,
        };
        let value = serde_json::to_value(core)?;
        receipt.receipt_digest = digest_canonical(FALLBACK_RECEIPT_DIGEST_DOMAIN, &value)
            .map_err(|error| CheckpointError::Canonical(error.to_string()))?;
        Ok(receipt)
    }
}

#[derive(Clone, Debug)]
pub struct LoadedCheckpointV1 {
    pub manifest: CheckpointManifestV1,
    pub disposition: CheckpointLoadDisposition,
    pub authority_receipt: CheckpointAuthorityValidationReceiptV1,
    pub fallback_receipt: Option<CheckpointFallbackReceiptV1>,
    directory: PathBuf,
    io_directory: PathBuf,
    store: Arc<CheckpointStoreInner>,
}

impl LoadedCheckpointV1 {
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn read_file(&self, logical_name: &str) -> Result<Vec<u8>, CheckpointError> {
        let _operation = self.store.operation_lock()?;
        let file = self
            .manifest
            .file_inventory
            .iter()
            .find(|file| file.logical_name == logical_name)
            .ok_or_else(|| CheckpointError::UnknownLogicalFile(logical_name.to_string()))?;
        let path = self.io_directory.join(&file.blob_path);
        let bytes = read_regular_file_no_follow(&path, None)?;
        if bytes.len() as u64 != file.byte_len || sha256_bytes(&bytes) != file.content_digest {
            return Err(CheckpointError::DigestMismatch {
                path,
                expected: file.content_digest.clone(),
                observed: sha256_bytes(&bytes),
            });
        }
        Ok(bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConfirmedCheckpointMarker;

#[derive(Clone, Debug, Serialize)]
pub struct CheckpointAckV1 {
    pub schema: String,
    pub checkpoint_id: String,
    pub brain_id: String,
    pub epoch: u64,
    pub generation: u64,
    pub revision: u64,
    pub current_pointer_digest: String,
    pub confirmed_at_unix_ms: u64,
    #[serde(skip)]
    _confirmed: ConfirmedCheckpointMarker,
}

impl CheckpointAckV1 {
    pub fn eviction_permit(
        &self,
        brain_id: &str,
        epoch: u64,
        generation: u64,
        revision: u64,
    ) -> Result<CheckpointEvictionPermitV1, CheckpointError> {
        if self.brain_id != brain_id
            || self.epoch != epoch
            || self.generation != generation
            || self.revision != revision
        {
            return Err(CheckpointError::EvictionAckMismatch {
                checkpoint_id: self.checkpoint_id.clone(),
            });
        }
        Ok(CheckpointEvictionPermitV1 {
            schema: CHECKPOINT_EVICTION_PERMIT_SCHEMA.to_string(),
            checkpoint_id: self.checkpoint_id.clone(),
            brain_id: self.brain_id.clone(),
            epoch: self.epoch,
            generation: self.generation,
            revision: self.revision,
            current_pointer_digest: self.current_pointer_digest.clone(),
            _confirmed: ConfirmedCheckpointMarker,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CheckpointEvictionPermitV1 {
    pub schema: String,
    pub checkpoint_id: String,
    pub brain_id: String,
    pub epoch: u64,
    pub generation: u64,
    pub revision: u64,
    pub current_pointer_digest: String,
    #[serde(skip)]
    _confirmed: ConfirmedCheckpointMarker,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointGcPolicyV1 {
    pub retain_newest_additional: usize,
    pub protected_checkpoint_ids: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointGcReceiptV1 {
    pub schema: String,
    pub current_pointer_digest: String,
    pub current_checkpoint_id: String,
    pub fallback_checkpoint_id: Option<String>,
    pub preserved_checkpoint_ids: Vec<String>,
    pub deleted_checkpoint_ids: Vec<String>,
    pub completed_at_unix_ms: u64,
    pub receipt_digest: String,
}

#[derive(Serialize)]
struct GcReceiptCore<'a> {
    schema: &'a str,
    current_pointer_digest: &'a str,
    current_checkpoint_id: &'a str,
    fallback_checkpoint_id: &'a Option<String>,
    preserved_checkpoint_ids: &'a [String],
    deleted_checkpoint_ids: &'a [String],
    completed_at_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckpointFaultPoint {
    CreateStagingDirectory,
    CreateBlobDirectory,
    WriteBlob,
    FsyncBlob,
    FsyncBlobDirectory,
    WriteManifest,
    FsyncManifest,
    FsyncStagingDirectory,
    RenameCheckpointDirectory,
    FsyncCheckpointParent,
    WriteCurrent,
    FsyncCurrent,
    RenameCurrent,
    FsyncCurrentParent,
    ConfirmCurrent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointFaultEvent {
    pub point: CheckpointFaultPoint,
    pub ordinal: u64,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InjectedCheckpointFault {
    pub code: String,
    pub detail: String,
}

impl InjectedCheckpointFault {
    pub fn new(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            detail: detail.into(),
        }
    }
}

pub trait CheckpointFaultInjector: Send + Sync {
    fn check(&self, event: &CheckpointFaultEvent) -> Result<(), InjectedCheckpointFault>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoCheckpointFaults;

impl CheckpointFaultInjector for NoCheckpointFaults {
    fn check(&self, _event: &CheckpointFaultEvent) -> Result<(), InjectedCheckpointFault> {
        Ok(())
    }
}

struct FaultCursor<'a> {
    injector: &'a dyn CheckpointFaultInjector,
    ordinal: u64,
}

impl<'a> FaultCursor<'a> {
    fn new(injector: &'a dyn CheckpointFaultInjector) -> Self {
        Self {
            injector,
            ordinal: 0,
        }
    }

    fn check(&mut self, point: CheckpointFaultPoint, path: &Path) -> Result<(), CheckpointError> {
        let event = CheckpointFaultEvent {
            point,
            ordinal: self.ordinal,
            path: path.to_path_buf(),
        };
        self.ordinal = self.ordinal.saturating_add(1);
        match catch_unwind(AssertUnwindSafe(|| self.injector.check(&event))) {
            Ok(result) => result.map_err(|fault| CheckpointError::Injected {
                point,
                code: fault.code,
                detail: fault.detail,
            }),
            Err(payload) => Err(CheckpointError::Injected {
                point,
                code: "checkpoint_fault_injector_panicked".to_string(),
                detail: panic_payload_detail(payload),
            }),
        }
    }
}

fn panic_payload_detail(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

#[derive(Debug)]
pub enum CheckpointError {
    Io(io::Error),
    Json(serde_json::Error),
    Canonical(String),
    Refused {
        code: &'static str,
        detail: String,
    },
    Injected {
        point: CheckpointFaultPoint,
        code: String,
        detail: String,
    },
    WriterLocked(String),
    SymlinkRefused(PathBuf),
    PointerMissing,
    PointerCorrupt(String),
    OccConflict {
        expected: Option<String>,
        observed: Option<String>,
    },
    DigestMismatch {
        path: PathBuf,
        expected: String,
        observed: String,
    },
    CheckpointCollision(String),
    UnknownLogicalFile(String),
    NoUsableCheckpoint {
        current_error: String,
        fallback_error: String,
    },
    AuthorityValidation(String),
    EvictionAckMismatch {
        checkpoint_id: String,
    },
    PlatformNotProven(&'static str),
    OperationPoisoned,
}

impl CheckpointError {
    fn refused(code: &'static str, detail: impl Into<String>) -> Self {
        Self::Refused {
            code,
            detail: detail.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "checkpoint_io",
            Self::Json(_) => "checkpoint_json",
            Self::Canonical(_) => "checkpoint_canonicalization",
            Self::Refused { code, .. } => code,
            Self::Injected { .. } => "checkpoint_fault_injected",
            Self::WriterLocked(_) => "checkpoint_writer_locked",
            Self::SymlinkRefused(_) => "checkpoint_symlink_refused",
            Self::PointerMissing => "checkpoint_pointer_missing",
            Self::PointerCorrupt(_) => "checkpoint_pointer_corrupt",
            Self::OccConflict { .. } => "checkpoint_occ_conflict",
            Self::DigestMismatch { .. } => "checkpoint_digest_mismatch",
            Self::CheckpointCollision(_) => "checkpoint_id_collision",
            Self::UnknownLogicalFile(_) => "checkpoint_unknown_logical_file",
            Self::NoUsableCheckpoint { .. } => "checkpoint_no_usable_generation",
            Self::AuthorityValidation(_) => "checkpoint_authority_validation_failed",
            Self::EvictionAckMismatch { .. } => "checkpoint_eviction_ack_mismatch",
            Self::PlatformNotProven(_) => "checkpoint_platform_not_proven",
            Self::OperationPoisoned => "checkpoint_operation_lock_poisoned",
        }
    }
}

impl fmt::Display for CheckpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "checkpoint I/O error: {error}"),
            Self::Json(error) => write!(formatter, "checkpoint JSON error: {error}"),
            Self::Canonical(error) => write!(formatter, "checkpoint canonicalization: {error}"),
            Self::Refused { code, detail } => write!(formatter, "{code}: {detail}"),
            Self::Injected {
                point,
                code,
                detail,
            } => write!(
                formatter,
                "injected checkpoint fault at {point:?} ({code}): {detail}"
            ),
            Self::WriterLocked(detail) => write!(formatter, "checkpoint writer locked: {detail}"),
            Self::SymlinkRefused(path) => {
                write!(formatter, "checkpoint symlink refused: {}", path.display())
            }
            Self::PointerMissing => formatter.write_str("checkpoint CURRENT pointer is absent"),
            Self::PointerCorrupt(detail) => {
                write!(formatter, "checkpoint CURRENT pointer is corrupt: {detail}")
            }
            Self::OccConflict { expected, observed } => write!(
                formatter,
                "checkpoint CURRENT conflict: expected {expected:?}, observed {observed:?}"
            ),
            Self::DigestMismatch {
                path,
                expected,
                observed,
            } => write!(
                formatter,
                "checkpoint digest mismatch for {}: expected {expected}, observed {observed}",
                path.display()
            ),
            Self::CheckpointCollision(id) => {
                write!(formatter, "checkpoint directory collision for {id}")
            }
            Self::UnknownLogicalFile(name) => {
                write!(formatter, "checkpoint has no logical file '{name}'")
            }
            Self::NoUsableCheckpoint {
                current_error,
                fallback_error,
            } => write!(
                formatter,
                "no usable checkpoint: current={current_error}; fallback={fallback_error}"
            ),
            Self::AuthorityValidation(detail) => {
                write!(
                    formatter,
                    "checkpoint authority validation failed: {detail}"
                )
            }
            Self::EvictionAckMismatch { checkpoint_id } => write!(
                formatter,
                "checkpoint {checkpoint_id} does not acknowledge the requested eviction revision"
            ),
            Self::PlatformNotProven(detail) => {
                write!(
                    formatter,
                    "checkpoint platform primitive not proven: {detail}"
                )
            }
            Self::OperationPoisoned => {
                formatter.write_str("checkpoint operation mutex was poisoned")
            }
        }
    }
}

impl Error for CheckpointError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for CheckpointError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for CheckpointError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

struct BuiltCheckpoint {
    manifest: CheckpointManifestV1,
    blobs: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectoryIdentity {
    device: u64,
    inode: u64,
}

#[derive(Debug)]
struct CheckpointStoreInner {
    namespace_root: PathBuf,
    checkpoints: PathBuf,
    parent: PathBuf,
    parent_identity: DirectoryIdentity,
    namespace_lease_path: PathBuf,
    namespace_lease_identity: DirectoryIdentity,
    root_identity: DirectoryIdentity,
    parent_directory: File,
    namespace_lease: File,
    root_lease: File,
    writer_lock: File,
    operation: Mutex<()>,
}

impl CheckpointStoreInner {
    fn operation_lock(&self) -> Result<MutexGuard<'_, ()>, CheckpointError> {
        let guard = self
            .operation
            .lock()
            .map_err(|_| CheckpointError::OperationPoisoned)?;
        self.verify_namespace_binding()?;
        Ok(guard)
    }

    fn verify_namespace_binding(&self) -> Result<(), CheckpointError> {
        verify_directory_binding(
            "checkpoint_parent_binding_changed",
            &self.parent,
            self.parent_identity,
        )?;
        verify_directory_binding(
            "checkpoint_namespace_lease_binding_changed",
            &self.namespace_lease_path,
            self.namespace_lease_identity,
        )?;
        verify_directory_binding(
            "checkpoint_root_binding_changed",
            &self.namespace_root,
            self.root_identity,
        )?;

        let parent_descriptor_identity = directory_identity_from_file(&self.parent_directory)?;
        if parent_descriptor_identity != self.parent_identity {
            return Err(CheckpointError::refused(
                "checkpoint_parent_descriptor_binding_changed",
                format!(
                    "parent descriptor identity changed from {:?} to {:?}",
                    self.parent_identity, parent_descriptor_identity
                ),
            ));
        }
        let lease_descriptor_identity = directory_identity_from_file(&self.namespace_lease)?;
        if lease_descriptor_identity != self.namespace_lease_identity {
            return Err(CheckpointError::refused(
                "checkpoint_namespace_lease_descriptor_binding_changed",
                format!(
                    "namespace lease descriptor identity changed from {:?} to {:?}",
                    self.namespace_lease_identity, lease_descriptor_identity
                ),
            ));
        }
        let descriptor_identity = directory_identity_from_file(&self.root_lease)?;
        if descriptor_identity != self.root_identity {
            return Err(CheckpointError::refused(
                "checkpoint_root_descriptor_binding_changed",
                format!(
                    "root descriptor identity changed from {:?} to {:?}",
                    self.root_identity, descriptor_identity
                ),
            ));
        }
        Ok(())
    }
}

impl Drop for CheckpointStoreInner {
    fn drop(&mut self) {
        unlock_file(&self.writer_lock);
        #[cfg(unix)]
        {
            unlock_file(&self.root_lease);
            unlock_file(&self.namespace_lease);
        }
    }
}

#[derive(Clone)]
pub struct CheckpointStore {
    inner: Arc<CheckpointStoreInner>,
}

impl CheckpointStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, CheckpointError> {
        let root = root.as_ref().to_path_buf();
        let (parent, namespace_root) = normalize_checkpoint_root(&root)?;

        // Pin the parent namespace before creating anchors. Unix also locks the
        // directory during creation; Windows holds a no-follow handle without
        // FILE_SHARE_DELETE, which prevents replacement while bindings settle.
        let parent_directory = open_directory_no_follow(&parent)?;
        #[cfg(unix)]
        lock_file_exclusive(&parent_directory)?;
        let parent_identity = directory_identity_from_file(&parent_directory)?;
        verify_directory_binding(
            "checkpoint_parent_binding_changed",
            &parent,
            parent_identity,
        )?;

        // This per-root lease lives beside the root, not inside it. Renaming
        // and recreating the root therefore cannot mint a fresh writer lease.
        let namespace_lease_path = parent.join(namespace_lease_name(&namespace_root));
        refuse_symlink_if_present(&namespace_lease_path)?;
        fs::create_dir_all(&namespace_lease_path)?;
        require_directory(&namespace_lease_path)?;
        let namespace_lease = open_directory_no_follow(&namespace_lease_path)?;
        #[cfg(unix)]
        lock_file_exclusive(&namespace_lease)?;
        let namespace_lease_identity = directory_identity_from_file(&namespace_lease)?;
        verify_directory_binding(
            "checkpoint_namespace_lease_binding_changed",
            &namespace_lease_path,
            namespace_lease_identity,
        )?;
        #[cfg(unix)]
        unlock_file(&parent_directory);

        refuse_symlink_if_present(&namespace_root)?;
        fs::create_dir_all(&namespace_root)?;
        require_directory(&namespace_root)?;

        // Pin the exact root identity for the writer lifetime. Unix locks this
        // directory descriptor; Windows denies delete sharing on the open root
        // handle. Cross-process writer exclusion itself is the separately held
        // WRITER.lock (`flock` / `LockFileEx`).
        let root_lease = open_directory_no_follow(&namespace_root)?;
        #[cfg(unix)]
        lock_file_exclusive(&root_lease)?;
        let root_identity = directory_identity_from_file(&root_lease)?;
        verify_directory_binding(
            "checkpoint_root_binding_changed",
            &namespace_root,
            root_identity,
        )?;

        let checkpoints = namespace_root.join(CHECKPOINTS_DIRECTORY);
        refuse_symlink_if_present(&checkpoints)?;
        fs::create_dir_all(&checkpoints)?;
        require_directory(&checkpoints)?;

        for sensitive in [
            namespace_root.join(CURRENT_FILE),
            namespace_root.join(WRITER_LOCK_FILE),
        ] {
            refuse_symlink_if_present(&sensitive)?;
        }

        let writer_lock = open_append_no_follow(&namespace_root.join(WRITER_LOCK_FILE))?;
        lock_file_exclusive(&writer_lock)?;
        cleanup_unpublished_temporaries(&namespace_root, &checkpoints)?;
        #[cfg(unix)]
        {
            sync_directory(&checkpoints)?;
            sync_directory(&namespace_root)?;
        }

        let store = Self {
            inner: Arc::new(CheckpointStoreInner {
                namespace_root,
                checkpoints,
                parent,
                parent_identity,
                namespace_lease_path,
                namespace_lease_identity,
                root_identity,
                parent_directory,
                namespace_lease,
                root_lease,
                writer_lock,
                operation: Mutex::new(()),
            }),
        };
        store.inner.verify_namespace_binding()?;
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.inner.namespace_root
    }

    pub fn checkpoint_directory(&self, checkpoint_id: &str) -> PathBuf {
        self.inner.checkpoints.join(checkpoint_id)
    }

    fn io_checkpoint_directory(&self, checkpoint_id: &str) -> PathBuf {
        self.inner.checkpoints.join(checkpoint_id)
    }

    pub fn create_checkpoint(
        &self,
        input: CheckpointCreateV1,
        injector: &dyn CheckpointFaultInjector,
    ) -> Result<CheckpointAckV1, CheckpointError> {
        let _operation = self.operation_lock()?;
        let built = build_checkpoint(input)?;
        let checkpoint_id = built.manifest.checkpoint_id.clone();
        let mut faults = FaultCursor::new(injector);
        let pointer_before = self.read_current_optional()?;

        if pointer_before
            .as_ref()
            .is_some_and(|pointer| pointer.current_checkpoint_id == checkpoint_id)
        {
            self.validate_checkpoint_directory(&checkpoint_id)?;
            faults.check(
                CheckpointFaultPoint::FsyncCurrentParent,
                &self.inner.namespace_root,
            )?;
            #[cfg(unix)]
            sync_directory(&self.inner.namespace_root)?;
            return self.confirm_ack(&built.manifest, &mut faults);
        }

        let observed = pointer_before
            .as_ref()
            .map(|pointer| pointer.current_checkpoint_id.clone());
        if observed != built.manifest.previous_checkpoint_id {
            return Err(CheckpointError::OccConflict {
                expected: built.manifest.previous_checkpoint_id.clone(),
                observed,
            });
        }
        if let Some(previous) = &built.manifest.previous_checkpoint_id {
            let previous_manifest = self.validate_checkpoint_directory(previous)?;
            validate_monotonic_successor(&previous_manifest, &built.manifest)?;
        }

        let final_directory = self.io_checkpoint_directory(&checkpoint_id);
        if final_directory.exists() {
            let existing = self.validate_checkpoint_directory(&checkpoint_id)?;
            if existing != built.manifest {
                return Err(CheckpointError::CheckpointCollision(checkpoint_id));
            }
        } else {
            self.write_immutable_checkpoint(&built, &mut faults)?;
        }

        faults.check(
            CheckpointFaultPoint::FsyncCheckpointParent,
            &self.inner.checkpoints,
        )?;
        #[cfg(unix)]
        sync_directory(&self.inner.checkpoints)?;

        let pointer = build_next_pointer(pointer_before.as_ref(), &built.manifest)?;
        self.publish_current(&pointer, &mut faults)?;
        self.confirm_ack(&built.manifest, &mut faults)
    }

    pub fn current_pointer(&self) -> Result<CheckpointCurrentV1, CheckpointError> {
        let _operation = self.operation_lock()?;
        self.read_current_required()
    }

    /// Re-establish the durability/confirmation half of a checkpoint whose
    /// immutable manifest may already be selected by CURRENT. This never
    /// chooses a generation: the caller supplies the exact expected manifest,
    /// and the method refuses unless CURRENT binds to it byte-for-byte. It is
    /// safe to retry after `rename(CURRENT)` when fsync/readback was interrupted.
    pub(crate) fn reconcile_current_manifest(
        &self,
        expected: &CheckpointManifestV1,
        injector: &dyn CheckpointFaultInjector,
    ) -> Result<CheckpointAckV1, CheckpointError> {
        let _operation = self.operation_lock()?;
        let pointer = self.read_current_required()?;
        if pointer.current_checkpoint_id != expected.checkpoint_id {
            return Err(CheckpointError::OccConflict {
                expected: Some(expected.checkpoint_id.clone()),
                observed: Some(pointer.current_checkpoint_id),
            });
        }
        validate_pointer_manifest_binding(&pointer, expected)?;
        let confirmed = self.validate_checkpoint_directory(&expected.checkpoint_id)?;
        if &confirmed != expected {
            return Err(CheckpointError::CheckpointCollision(
                expected.checkpoint_id.clone(),
            ));
        }
        let mut faults = FaultCursor::new(injector);
        faults.check(
            CheckpointFaultPoint::FsyncCurrentParent,
            &self.inner.namespace_root,
        )?;
        #[cfg(unix)]
        sync_directory(&self.inner.namespace_root)?;
        self.confirm_ack(expected, &mut faults)
    }

    pub fn load_current(
        &self,
        validator: &dyn CheckpointAuthorityValidator,
    ) -> Result<LoadedCheckpointV1, CheckpointError> {
        let _operation = self.operation_lock()?;
        let pointer = self.read_current_required()?;
        self.load_checkpoint(
            &pointer.current_checkpoint_id,
            CheckpointLoadDisposition::ExactCurrent,
            Some(&pointer),
            validator,
            None,
        )
    }

    /// Read the exact bytes of a manifest that was already accepted by the
    /// brain actor.  This is intentionally narrower than `load_current`: it
    /// does not manufacture a new authority receipt and it refuses if the
    /// immutable directory no longer matches the cached manifest.  The actor
    /// uses it only to restore its canonical working files after a mutation
    /// failed *before* publishing a successor CURRENT pointer.
    pub(crate) fn read_verified_manifest_files(
        &self,
        expected: &CheckpointManifestV1,
    ) -> Result<Vec<(CheckpointFileV1, Vec<u8>)>, CheckpointError> {
        let _operation = self.operation_lock()?;
        let observed = self.validate_checkpoint_directory(&expected.checkpoint_id)?;
        if &observed != expected {
            return Err(CheckpointError::CheckpointCollision(
                expected.checkpoint_id.clone(),
            ));
        }
        let directory = self.io_checkpoint_directory(&expected.checkpoint_id);
        expected
            .file_inventory
            .iter()
            .map(|file| {
                let path = directory.join(&file.blob_path);
                let bytes = read_regular_file_no_follow(&path, Some(file.byte_len))?;
                let observed_digest = sha256_bytes(&bytes);
                if bytes.len() as u64 != file.byte_len || observed_digest != file.content_digest {
                    return Err(CheckpointError::DigestMismatch {
                        path,
                        expected: file.content_digest.clone(),
                        observed: observed_digest,
                    });
                }
                Ok((file.clone(), bytes))
            })
            .collect()
    }

    /// Validate and return one immutable manifest by content-addressed id
    /// without creating an authority receipt. Recovery uses the immediate
    /// predecessor's owned-path inventory only to remove stale post-commit
    /// projections after a crash; the selected generation remains the sole
    /// source of restored bytes.
    pub(crate) fn read_verified_manifest(
        &self,
        checkpoint_id: &str,
    ) -> Result<CheckpointManifestV1, CheckpointError> {
        let _operation = self.operation_lock()?;
        self.validate_checkpoint_directory(checkpoint_id)
    }

    /// Authenticate one content-addressed manifest and read exactly one of its
    /// declared blobs. Fallback recovery uses this narrow operation for the
    /// rejected CURRENT's working-set metadata: another corrupt blob may make
    /// that generation unusable, but neither an unauthorized manifest nor a
    /// corrupt working-set blob may authorize destructive path cleanup.
    pub(crate) fn read_authorized_manifest_file(
        &self,
        checkpoint_id: &str,
        logical_name: &str,
        validator: &dyn CheckpointAuthorityValidator,
    ) -> Result<(CheckpointManifestV1, Vec<u8>), CheckpointError> {
        let _operation = self.operation_lock()?;
        let manifest = self.validate_content_addressed_manifest(checkpoint_id)?;
        let refs_digest = external_authority_refs_digest(&manifest.external_authority_refs)?;
        let receipt = validator
            .validate(&manifest, &refs_digest)
            .map_err(CheckpointError::AuthorityValidation)?;
        validate_authority_receipt(&receipt, &manifest, &refs_digest)?;
        let file = manifest
            .file_inventory
            .iter()
            .find(|file| file.logical_name == logical_name)
            .ok_or_else(|| CheckpointError::UnknownLogicalFile(logical_name.to_string()))?;
        let path = self
            .io_checkpoint_directory(checkpoint_id)
            .join(&file.blob_path);
        let bytes = read_regular_file_no_follow(&path, None)?;
        let observed = sha256_bytes(&bytes);
        if bytes.len() as u64 != file.byte_len || observed != file.content_digest {
            return Err(CheckpointError::DigestMismatch {
                path,
                expected: file.content_digest.clone(),
                observed,
            });
        }
        Ok((manifest, bytes))
    }

    pub fn load_with_fallback(
        &self,
        validator: &dyn CheckpointAuthorityValidator,
        issued_at_unix_ms: u64,
    ) -> Result<LoadedCheckpointV1, CheckpointError> {
        let _operation = self.operation_lock()?;
        let pointer = self.read_current_required()?;
        match self.load_checkpoint(
            &pointer.current_checkpoint_id,
            CheckpointLoadDisposition::ExactCurrent,
            Some(&pointer),
            validator,
            None,
        ) {
            Ok(loaded) => Ok(loaded),
            Err(current_error @ CheckpointError::PointerCorrupt(_)) => Err(current_error),
            Err(current_error) => {
                let Some(fallback_id) = pointer.fallback_checkpoint_id.clone() else {
                    return Err(CheckpointError::NoUsableCheckpoint {
                        current_error: current_error.to_string(),
                        fallback_error: "CURRENT declares no fallback checkpoint".to_string(),
                    });
                };
                let receipt = CheckpointFallbackReceiptV1::new(
                    pointer.current_checkpoint_id.clone(),
                    fallback_id.clone(),
                    pointer.pointer_digest.clone(),
                    current_error.code().to_string(),
                    &current_error.to_string(),
                    issued_at_unix_ms,
                )?;
                self.load_checkpoint(
                    &fallback_id,
                    CheckpointLoadDisposition::DegradedFallback,
                    None,
                    validator,
                    Some(receipt),
                )
                .map_err(|fallback_error| CheckpointError::NoUsableCheckpoint {
                    current_error: current_error.to_string(),
                    fallback_error: fallback_error.to_string(),
                })
            }
        }
    }

    pub fn gc(
        &self,
        policy: &CheckpointGcPolicyV1,
    ) -> Result<CheckpointGcReceiptV1, CheckpointError> {
        let _operation = self.operation_lock()?;
        for checkpoint_id in &policy.protected_checkpoint_ids {
            validate_digest("protected_checkpoint_id", checkpoint_id)?;
            self.require_checkpoint_directory_present(checkpoint_id)?;
        }
        let pointer = self.read_current_required()?;
        self.require_checkpoint_directory_present(&pointer.current_checkpoint_id)?;
        let fallback_predecessor = if let Some(fallback) = &pointer.fallback_checkpoint_id {
            self.require_checkpoint_directory_present(fallback)?;
            self.validate_content_addressed_manifest(fallback)?
                .previous_checkpoint_id
        } else {
            None
        };
        let mut preserved = policy.protected_checkpoint_ids.clone();
        preserved.insert(pointer.current_checkpoint_id.clone());
        if let Some(fallback) = &pointer.fallback_checkpoint_id {
            preserved.insert(fallback.clone());
        }
        // Legacy fallback generations predate the self-contained working-set
        // envelope and need their immediate predecessor to discover paths that
        // must be removed. Retaining one extra immutable generation is cheap
        // and keeps both legacy recovery and forward migration available.
        if let Some(predecessor) = fallback_predecessor {
            self.require_checkpoint_directory_present(&predecessor)?;
            preserved.insert(predecessor);
        }

        let mut candidates = Vec::new();
        for entry in fs::read_dir(&self.inner.checkpoints)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata_is_link_or_reparse(&metadata) {
                return Err(CheckpointError::SymlinkRefused(path));
            }
            if !metadata.is_dir() || !is_digest(&name) {
                continue;
            }
            let order = self
                .validate_checkpoint_directory(&name)
                .map(|manifest| {
                    (
                        manifest.epoch,
                        manifest.generation,
                        manifest.revision,
                        manifest.created_at_unix_ms,
                    )
                })
                .unwrap_or((0, 0, 0, 0));
            candidates.push((order, name));
        }
        candidates.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        let mut retained_additional = 0;
        for (_, checkpoint_id) in &candidates {
            if retained_additional >= policy.retain_newest_additional {
                break;
            }
            if preserved.insert(checkpoint_id.clone()) {
                retained_additional += 1;
            }
        }

        let mut deleted = Vec::new();
        for (_, checkpoint_id) in candidates {
            if preserved.contains(&checkpoint_id) {
                continue;
            }
            let current_now = self.read_current_required()?;
            if current_now.pointer_digest != pointer.pointer_digest {
                return Err(CheckpointError::OccConflict {
                    expected: Some(pointer.current_checkpoint_id.clone()),
                    observed: Some(current_now.current_checkpoint_id),
                });
            }
            let path = self.io_checkpoint_directory(&checkpoint_id);
            let metadata = fs::symlink_metadata(&path)?;
            if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
                return Err(CheckpointError::SymlinkRefused(path));
            }
            retire_checkpoint_directory(&path, &self.inner.checkpoints, &checkpoint_id)?;
            deleted.push(checkpoint_id);
        }
        #[cfg(unix)]
        sync_directory(&self.inner.checkpoints)?;

        let mut preserved_checkpoint_ids = preserved.into_iter().collect::<Vec<_>>();
        preserved_checkpoint_ids.sort();
        deleted.sort();
        let completed_at_unix_ms = now_unix_ms()?;
        let mut receipt = CheckpointGcReceiptV1 {
            schema: CHECKPOINT_GC_RECEIPT_SCHEMA.to_string(),
            current_pointer_digest: pointer.pointer_digest.clone(),
            current_checkpoint_id: pointer.current_checkpoint_id.clone(),
            fallback_checkpoint_id: pointer.fallback_checkpoint_id.clone(),
            preserved_checkpoint_ids,
            deleted_checkpoint_ids: deleted,
            completed_at_unix_ms,
            receipt_digest: String::new(),
        };
        let core = GcReceiptCore {
            schema: &receipt.schema,
            current_pointer_digest: &receipt.current_pointer_digest,
            current_checkpoint_id: &receipt.current_checkpoint_id,
            fallback_checkpoint_id: &receipt.fallback_checkpoint_id,
            preserved_checkpoint_ids: &receipt.preserved_checkpoint_ids,
            deleted_checkpoint_ids: &receipt.deleted_checkpoint_ids,
            completed_at_unix_ms: receipt.completed_at_unix_ms,
        };
        let value = serde_json::to_value(core)?;
        receipt.receipt_digest = digest_canonical(GC_RECEIPT_DIGEST_DOMAIN, &value)
            .map_err(|error| CheckpointError::Canonical(error.to_string()))?;
        Ok(receipt)
    }

    fn operation_lock(&self) -> Result<MutexGuard<'_, ()>, CheckpointError> {
        self.inner.operation_lock()
    }

    fn write_immutable_checkpoint(
        &self,
        built: &BuiltCheckpoint,
        faults: &mut FaultCursor<'_>,
    ) -> Result<(), CheckpointError> {
        let nonce = TEMP_NONCE.fetch_add(1, Ordering::Relaxed);
        let staging = self.inner.checkpoints.join(format!(
            ".staging-{}-{}-{nonce}",
            std::process::id(),
            &built.manifest.checkpoint_id[..12]
        ));
        faults.check(CheckpointFaultPoint::CreateStagingDirectory, &staging)?;
        fs::create_dir(&staging)?;
        let mut staging_guard = DirectoryCleanupGuard::new(staging.clone());

        let blobs_directory = staging.join(BLOBS_DIRECTORY);
        faults.check(CheckpointFaultPoint::CreateBlobDirectory, &blobs_directory)?;
        fs::create_dir(&blobs_directory)?;
        for (digest, bytes) in &built.blobs {
            let path = blobs_directory.join(digest);
            write_new_file(
                &path,
                bytes,
                CheckpointFaultPoint::WriteBlob,
                CheckpointFaultPoint::FsyncBlob,
                faults,
            )?;
        }
        faults.check(CheckpointFaultPoint::FsyncBlobDirectory, &blobs_directory)?;
        #[cfg(unix)]
        sync_directory(&blobs_directory)?;

        let manifest_path = staging.join(MANIFEST_FILE);
        let manifest_bytes = serde_json::to_vec_pretty(&built.manifest)?;
        write_new_file(
            &manifest_path,
            &manifest_bytes,
            CheckpointFaultPoint::WriteManifest,
            CheckpointFaultPoint::FsyncManifest,
            faults,
        )?;
        faults.check(CheckpointFaultPoint::FsyncStagingDirectory, &staging)?;
        #[cfg(unix)]
        sync_directory(&staging)?;

        let final_directory = self.io_checkpoint_directory(&built.manifest.checkpoint_id);
        faults.check(
            CheckpointFaultPoint::RenameCheckpointDirectory,
            &final_directory,
        )?;
        publish_new_path(&staging, &final_directory)?;
        staging_guard.disarm();
        Ok(())
    }

    fn publish_current(
        &self,
        pointer: &CheckpointCurrentV1,
        faults: &mut FaultCursor<'_>,
    ) -> Result<(), CheckpointError> {
        let nonce = TEMP_NONCE.fetch_add(1, Ordering::Relaxed);
        let temporary = self
            .inner
            .namespace_root
            .join(format!(".CURRENT.tmp-{}-{nonce}", std::process::id()));
        let mut temporary_guard = FileCleanupGuard::new(temporary.clone());
        let bytes = serde_json::to_vec_pretty(pointer)?;
        write_new_file(
            &temporary,
            &bytes,
            CheckpointFaultPoint::WriteCurrent,
            CheckpointFaultPoint::FsyncCurrent,
            faults,
        )?;
        let current_path = self.inner.namespace_root.join(CURRENT_FILE);
        refuse_symlink_if_present(&current_path)?;
        faults.check(CheckpointFaultPoint::RenameCurrent, &current_path)?;
        replace_path(&temporary, &current_path)?;
        temporary_guard.disarm();
        faults.check(
            CheckpointFaultPoint::FsyncCurrentParent,
            &self.inner.namespace_root,
        )?;
        #[cfg(unix)]
        sync_directory(&self.inner.namespace_root)?;
        Ok(())
    }

    fn confirm_ack(
        &self,
        manifest: &CheckpointManifestV1,
        faults: &mut FaultCursor<'_>,
    ) -> Result<CheckpointAckV1, CheckpointError> {
        faults.check(
            CheckpointFaultPoint::ConfirmCurrent,
            &self.inner.namespace_root.join(CURRENT_FILE),
        )?;
        let pointer = self.read_current_required()?;
        if pointer.current_checkpoint_id != manifest.checkpoint_id {
            return Err(CheckpointError::OccConflict {
                expected: Some(manifest.checkpoint_id.clone()),
                observed: Some(pointer.current_checkpoint_id),
            });
        }
        validate_pointer_manifest_binding(&pointer, manifest)?;
        let confirmed = self.validate_checkpoint_directory(&manifest.checkpoint_id)?;
        if &confirmed != manifest {
            return Err(CheckpointError::CheckpointCollision(
                manifest.checkpoint_id.clone(),
            ));
        }
        Ok(CheckpointAckV1 {
            schema: CHECKPOINT_ACK_SCHEMA.to_string(),
            checkpoint_id: manifest.checkpoint_id.clone(),
            brain_id: manifest.brain_id.clone(),
            epoch: manifest.epoch,
            generation: manifest.generation,
            revision: manifest.revision,
            current_pointer_digest: pointer.pointer_digest,
            confirmed_at_unix_ms: now_unix_ms()?,
            _confirmed: ConfirmedCheckpointMarker,
        })
    }

    fn load_checkpoint(
        &self,
        checkpoint_id: &str,
        disposition: CheckpointLoadDisposition,
        pointer_binding: Option<&CheckpointCurrentV1>,
        validator: &dyn CheckpointAuthorityValidator,
        fallback_receipt: Option<CheckpointFallbackReceiptV1>,
    ) -> Result<LoadedCheckpointV1, CheckpointError> {
        let manifest = self.validate_checkpoint_directory(checkpoint_id)?;
        if let Some(pointer) = pointer_binding {
            validate_pointer_manifest_binding(pointer, &manifest)?;
        }
        let refs_digest = external_authority_refs_digest(&manifest.external_authority_refs)?;
        let authority_receipt = validator
            .validate(&manifest, &refs_digest)
            .map_err(CheckpointError::AuthorityValidation)?;
        validate_authority_receipt(&authority_receipt, &manifest, &refs_digest)?;
        Ok(LoadedCheckpointV1 {
            directory: self.checkpoint_directory(checkpoint_id),
            io_directory: self.io_checkpoint_directory(checkpoint_id),
            store: Arc::clone(&self.inner),
            manifest,
            disposition,
            authority_receipt,
            fallback_receipt,
        })
    }

    fn validate_checkpoint_directory(
        &self,
        checkpoint_id: &str,
    ) -> Result<CheckpointManifestV1, CheckpointError> {
        let manifest = self.validate_content_addressed_manifest(checkpoint_id)?;
        let directory = self.io_checkpoint_directory(checkpoint_id);

        let mut root_entries = BTreeSet::new();
        for entry in fs::read_dir(&directory)? {
            root_entries.insert(entry?.file_name().to_string_lossy().to_string());
        }
        let expected_root_entries =
            BTreeSet::from([MANIFEST_FILE.to_string(), BLOBS_DIRECTORY.to_string()]);
        if root_entries != expected_root_entries {
            return Err(CheckpointError::refused(
                "checkpoint_inventory_mismatch",
                format!("unexpected checkpoint root entries: {root_entries:?}"),
            ));
        }

        let blobs_directory = directory.join(BLOBS_DIRECTORY);
        let blobs_metadata = fs::symlink_metadata(&blobs_directory)?;
        if metadata_is_link_or_reparse(&blobs_metadata) || !blobs_metadata.is_dir() {
            return Err(CheckpointError::SymlinkRefused(blobs_directory));
        }
        let expected_blobs = manifest
            .file_inventory
            .iter()
            .map(|file| file.content_digest.clone())
            .collect::<BTreeSet<_>>();
        let mut observed_blobs = BTreeSet::new();
        for entry in fs::read_dir(&blobs_directory)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
                return Err(CheckpointError::SymlinkRefused(path));
            }
            observed_blobs.insert(name);
        }
        if observed_blobs != expected_blobs {
            return Err(CheckpointError::refused(
                "checkpoint_blob_inventory_mismatch",
                format!("expected {expected_blobs:?}, observed {observed_blobs:?}"),
            ));
        }
        for file in &manifest.file_inventory {
            let path = directory.join(&file.blob_path);
            let (digest, byte_len) = hash_regular_file_no_follow(&path)?;
            if digest != file.content_digest || byte_len != file.byte_len {
                return Err(CheckpointError::DigestMismatch {
                    path,
                    expected: file.content_digest.clone(),
                    observed: digest,
                });
            }
        }
        Ok(manifest)
    }

    fn validate_content_addressed_manifest(
        &self,
        checkpoint_id: &str,
    ) -> Result<CheckpointManifestV1, CheckpointError> {
        validate_digest("checkpoint_id", checkpoint_id)?;
        let directory = self.io_checkpoint_directory(checkpoint_id);
        let metadata = fs::symlink_metadata(&directory)?;
        if metadata_is_link_or_reparse(&metadata) {
            return Err(CheckpointError::SymlinkRefused(directory));
        }
        if !metadata.is_dir() {
            return Err(CheckpointError::refused(
                "checkpoint_not_directory",
                checkpoint_id,
            ));
        }

        let manifest_path = directory.join(MANIFEST_FILE);
        let manifest_bytes = read_regular_file_no_follow(&manifest_path, Some(MAX_MANIFEST_BYTES))?;
        let manifest: CheckpointManifestV1 = serde_json::from_slice(&manifest_bytes)?;
        validate_manifest(&manifest)?;
        if manifest.checkpoint_id != checkpoint_id {
            return Err(CheckpointError::CheckpointCollision(
                checkpoint_id.to_string(),
            ));
        }

        Ok(manifest)
    }

    fn require_checkpoint_directory_present(
        &self,
        checkpoint_id: &str,
    ) -> Result<(), CheckpointError> {
        validate_digest("checkpoint_id", checkpoint_id)?;
        let path = self.io_checkpoint_directory(checkpoint_id);
        let metadata = fs::symlink_metadata(&path)?;
        if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
            return Err(CheckpointError::SymlinkRefused(path));
        }
        Ok(())
    }

    fn read_current_optional(&self) -> Result<Option<CheckpointCurrentV1>, CheckpointError> {
        let path = self.inner.namespace_root.join(CURRENT_FILE);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata_is_link_or_reparse(&metadata) {
                    return Err(CheckpointError::SymlinkRefused(path));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        }
        let bytes = read_regular_file_no_follow(&path, Some(MAX_POINTER_BYTES))?;
        let pointer: CheckpointCurrentV1 = serde_json::from_slice(&bytes)
            .map_err(|error| CheckpointError::PointerCorrupt(error.to_string()))?;
        validate_pointer(&pointer)?;
        Ok(Some(pointer))
    }

    fn read_current_required(&self) -> Result<CheckpointCurrentV1, CheckpointError> {
        self.read_current_optional()?
            .ok_or(CheckpointError::PointerMissing)
    }
}

fn build_checkpoint(input: CheckpointCreateV1) -> Result<BuiltCheckpoint, CheckpointError> {
    validate_identifier("brain_id", &input.brain_id)?;
    if input.created_at_unix_ms == 0 {
        return Err(CheckpointError::refused(
            "checkpoint_created_at_missing",
            "created_at_unix_ms must be non-zero",
        ));
    }
    if let Some(previous) = &input.expected_current_checkpoint_id {
        validate_digest("expected_current_checkpoint_id", previous)?;
    }
    if input.files.is_empty() {
        return Err(CheckpointError::refused(
            "checkpoint_empty_inventory",
            "checkpoint must contain graph, ingest roots, and sidecars",
        ));
    }
    if input.schema_versions.is_empty() {
        return Err(CheckpointError::refused(
            "checkpoint_schema_pins_missing",
            "schema_versions cannot be empty",
        ));
    }
    for (schema_id, version) in &input.schema_versions {
        validate_identifier("schema_id", schema_id)?;
        validate_identifier("schema_version", version)?;
    }
    validate_external_authority_refs(&input.external_authority_refs)?;

    let mut logical_names = BTreeSet::new();
    let mut relative_paths = BTreeSet::new();
    let mut inventory = Vec::new();
    let mut blobs = BTreeMap::new();
    for file in input.files {
        validate_identifier("logical_name", &file.logical_name)?;
        validate_relative_logical_path(&file.relative_path)?;
        validate_identifier("file.schema_id", &file.schema_id)?;
        validate_identifier("file.schema_version", &file.schema_version)?;
        if input.schema_versions.get(&file.schema_id) != Some(&file.schema_version) {
            return Err(CheckpointError::refused(
                "checkpoint_schema_pin_mismatch",
                format!(
                    "{} declares {}={} but schema_versions does not pin it",
                    file.logical_name, file.schema_id, file.schema_version
                ),
            ));
        }
        if !logical_names.insert(file.logical_name.clone()) {
            return Err(CheckpointError::refused(
                "checkpoint_duplicate_logical_name",
                file.logical_name,
            ));
        }
        if !relative_paths.insert(file.relative_path.clone()) {
            return Err(CheckpointError::refused(
                "checkpoint_duplicate_relative_path",
                file.relative_path,
            ));
        }
        let digest = sha256_bytes(&file.bytes);
        let byte_len = file.bytes.len() as u64;
        blobs.entry(digest.clone()).or_insert(file.bytes);
        inventory.push(CheckpointFileV1 {
            schema: CHECKPOINT_FILE_SCHEMA.to_string(),
            logical_name: file.logical_name,
            relative_path: file.relative_path,
            schema_id: file.schema_id,
            schema_version: file.schema_version,
            content_digest: digest.clone(),
            byte_len,
            blob_path: format!("{BLOBS_DIRECTORY}/{digest}"),
        });
    }
    inventory.sort_by(|left, right| left.logical_name.cmp(&right.logical_name));
    let graph_snapshot_digest = inventory
        .iter()
        .find(|file| file.logical_name == GRAPH_SNAPSHOT_LOGICAL_NAME)
        .map(|file| file.content_digest.clone())
        .ok_or_else(|| {
            CheckpointError::refused(
                "checkpoint_graph_snapshot_missing",
                GRAPH_SNAPSHOT_LOGICAL_NAME,
            )
        })?;
    let ingest_roots_digest = inventory
        .iter()
        .find(|file| file.logical_name == INGEST_ROOTS_LOGICAL_NAME)
        .map(|file| file.content_digest.clone())
        .ok_or_else(|| {
            CheckpointError::refused("checkpoint_ingest_roots_missing", INGEST_ROOTS_LOGICAL_NAME)
        })?;
    let sidecar_digests = inventory
        .iter()
        .filter(|file| {
            file.logical_name != GRAPH_SNAPSHOT_LOGICAL_NAME
                && file.logical_name != INGEST_ROOTS_LOGICAL_NAME
        })
        .map(|file| (file.logical_name.clone(), file.content_digest.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut manifest = CheckpointManifestV1 {
        schema: CHECKPOINT_MANIFEST_SCHEMA.to_string(),
        checkpoint_id: String::new(),
        brain_id: input.brain_id,
        epoch: input.epoch,
        generation: input.generation,
        revision: input.revision,
        schema_versions: input.schema_versions,
        file_inventory: inventory,
        graph_snapshot_digest,
        sidecar_digests,
        ingest_roots_digest,
        external_authority_refs: input.external_authority_refs,
        created_at_unix_ms: input.created_at_unix_ms,
        previous_checkpoint_id: input.expected_current_checkpoint_id,
    };
    manifest.checkpoint_id = manifest.compute_checkpoint_id()?;
    validate_manifest(&manifest)?;
    Ok(BuiltCheckpoint { manifest, blobs })
}

fn validate_manifest(manifest: &CheckpointManifestV1) -> Result<(), CheckpointError> {
    if manifest.schema != CHECKPOINT_MANIFEST_SCHEMA {
        return Err(CheckpointError::refused(
            "checkpoint_manifest_schema_unsupported",
            &manifest.schema,
        ));
    }
    validate_digest("checkpoint_id", &manifest.checkpoint_id)?;
    validate_identifier("brain_id", &manifest.brain_id)?;
    if manifest.created_at_unix_ms == 0 || manifest.schema_versions.is_empty() {
        return Err(CheckpointError::refused(
            "checkpoint_manifest_incomplete",
            "created_at and schema pins are required",
        ));
    }
    for (schema_id, version) in &manifest.schema_versions {
        validate_identifier("schema_id", schema_id)?;
        validate_identifier("schema_version", version)?;
    }
    if let Some(previous) = &manifest.previous_checkpoint_id {
        validate_digest("previous_checkpoint_id", previous)?;
        if previous == &manifest.checkpoint_id {
            return Err(CheckpointError::refused("checkpoint_self_cycle", previous));
        }
    }
    validate_external_authority_refs(&manifest.external_authority_refs)?;
    let computed = manifest.compute_checkpoint_id()?;
    if computed != manifest.checkpoint_id {
        return Err(CheckpointError::DigestMismatch {
            path: PathBuf::from(MANIFEST_FILE),
            expected: manifest.checkpoint_id.clone(),
            observed: computed,
        });
    }
    let mut previous_logical_name: Option<&str> = None;
    let mut logical_names = BTreeSet::new();
    let mut relative_paths = BTreeSet::new();
    let mut derived_graph = None;
    let mut derived_ingest = None;
    let mut derived_sidecars = BTreeMap::new();
    for file in &manifest.file_inventory {
        if file.schema != CHECKPOINT_FILE_SCHEMA {
            return Err(CheckpointError::refused(
                "checkpoint_file_schema_unsupported",
                &file.schema,
            ));
        }
        validate_identifier("file.logical_name", &file.logical_name)?;
        validate_relative_logical_path(&file.relative_path)?;
        validate_identifier("file.schema_id", &file.schema_id)?;
        validate_identifier("file.schema_version", &file.schema_version)?;
        validate_digest("file.content_digest", &file.content_digest)?;
        if file.blob_path != format!("{BLOBS_DIRECTORY}/{}", file.content_digest) {
            return Err(CheckpointError::refused(
                "checkpoint_blob_path_mismatch",
                &file.blob_path,
            ));
        }
        if manifest.schema_versions.get(&file.schema_id) != Some(&file.schema_version) {
            return Err(CheckpointError::refused(
                "checkpoint_schema_pin_mismatch",
                &file.logical_name,
            ));
        }
        if previous_logical_name.is_some_and(|previous| previous >= file.logical_name.as_str()) {
            return Err(CheckpointError::refused(
                "checkpoint_inventory_not_sorted",
                &file.logical_name,
            ));
        }
        previous_logical_name = Some(&file.logical_name);
        if !logical_names.insert(file.logical_name.clone())
            || !relative_paths.insert(file.relative_path.clone())
        {
            return Err(CheckpointError::refused(
                "checkpoint_inventory_duplicate",
                &file.logical_name,
            ));
        }
        match file.logical_name.as_str() {
            GRAPH_SNAPSHOT_LOGICAL_NAME => derived_graph = Some(file.content_digest.clone()),
            INGEST_ROOTS_LOGICAL_NAME => derived_ingest = Some(file.content_digest.clone()),
            _ => {
                derived_sidecars.insert(file.logical_name.clone(), file.content_digest.clone());
            }
        }
    }
    if derived_graph.as_deref() != Some(manifest.graph_snapshot_digest.as_str())
        || derived_ingest.as_deref() != Some(manifest.ingest_roots_digest.as_str())
        || derived_sidecars != manifest.sidecar_digests
    {
        return Err(CheckpointError::refused(
            "checkpoint_derived_digest_mismatch",
            "graph, ingest roots, or sidecar digest projection differs from inventory",
        ));
    }
    Ok(())
}

fn validate_monotonic_successor(
    previous: &CheckpointManifestV1,
    next: &CheckpointManifestV1,
) -> Result<(), CheckpointError> {
    if previous.brain_id != next.brain_id {
        return Err(CheckpointError::refused(
            "checkpoint_brain_mismatch",
            format!("{} != {}", previous.brain_id, next.brain_id),
        ));
    }
    if (next.epoch, next.generation, next.revision)
        <= (previous.epoch, previous.generation, previous.revision)
    {
        return Err(CheckpointError::refused(
            "checkpoint_generation_not_monotonic",
            format!(
                "previous=({},{},{}), next=({},{},{})",
                previous.epoch,
                previous.generation,
                previous.revision,
                next.epoch,
                next.generation,
                next.revision
            ),
        ));
    }
    Ok(())
}

fn build_next_pointer(
    previous: Option<&CheckpointCurrentV1>,
    manifest: &CheckpointManifestV1,
) -> Result<CheckpointCurrentV1, CheckpointError> {
    let mut pointer = CheckpointCurrentV1 {
        schema: CHECKPOINT_CURRENT_SCHEMA.to_string(),
        pointer_revision: previous
            .map(|pointer| pointer.pointer_revision.saturating_add(1))
            .unwrap_or(1),
        current_checkpoint_id: manifest.checkpoint_id.clone(),
        fallback_checkpoint_id: previous.map(|pointer| pointer.current_checkpoint_id.clone()),
        previous_pointer_digest: previous.map(|pointer| pointer.pointer_digest.clone()),
        pointer_digest: String::new(),
    };
    pointer.pointer_digest = pointer.compute_digest()?;
    Ok(pointer)
}

fn validate_pointer(pointer: &CheckpointCurrentV1) -> Result<(), CheckpointError> {
    if pointer.schema != CHECKPOINT_CURRENT_SCHEMA {
        return Err(CheckpointError::PointerCorrupt(format!(
            "unsupported schema '{}'",
            pointer.schema
        )));
    }
    if pointer.pointer_revision == 0 {
        return Err(CheckpointError::PointerCorrupt(
            "pointer_revision is zero".to_string(),
        ));
    }
    validate_digest("current_checkpoint_id", &pointer.current_checkpoint_id)
        .map_err(|error| CheckpointError::PointerCorrupt(error.to_string()))?;
    if let Some(fallback) = &pointer.fallback_checkpoint_id {
        validate_digest("fallback_checkpoint_id", fallback)
            .map_err(|error| CheckpointError::PointerCorrupt(error.to_string()))?;
        if fallback == &pointer.current_checkpoint_id {
            return Err(CheckpointError::PointerCorrupt(
                "current and fallback checkpoint ids are identical".to_string(),
            ));
        }
    }
    if let Some(previous_digest) = &pointer.previous_pointer_digest {
        validate_digest("previous_pointer_digest", previous_digest)
            .map_err(|error| CheckpointError::PointerCorrupt(error.to_string()))?;
    }
    validate_digest("pointer_digest", &pointer.pointer_digest)
        .map_err(|error| CheckpointError::PointerCorrupt(error.to_string()))?;
    let computed = pointer
        .compute_digest()
        .map_err(|error| CheckpointError::PointerCorrupt(error.to_string()))?;
    if computed != pointer.pointer_digest {
        return Err(CheckpointError::PointerCorrupt(format!(
            "digest mismatch: expected {}, observed {computed}",
            pointer.pointer_digest
        )));
    }
    Ok(())
}

fn validate_pointer_manifest_binding(
    pointer: &CheckpointCurrentV1,
    manifest: &CheckpointManifestV1,
) -> Result<(), CheckpointError> {
    if pointer.current_checkpoint_id != manifest.checkpoint_id {
        return Err(CheckpointError::PointerCorrupt(format!(
            "current checkpoint {} does not bind manifest {}",
            pointer.current_checkpoint_id, manifest.checkpoint_id
        )));
    }
    if pointer.fallback_checkpoint_id != manifest.previous_checkpoint_id {
        return Err(CheckpointError::PointerCorrupt(format!(
            "fallback {:?} does not bind manifest predecessor {:?}",
            pointer.fallback_checkpoint_id, manifest.previous_checkpoint_id
        )));
    }
    Ok(())
}

pub fn external_authority_refs_digest(
    refs: &CheckpointExternalAuthorityRefsV1,
) -> Result<String, CheckpointError> {
    validate_external_authority_refs(refs)?;
    let value = serde_json::to_value(refs)?;
    digest_canonical(EXTERNAL_AUTHORITY_REFS_DIGEST_DOMAIN, &value)
        .map_err(|error| CheckpointError::Canonical(error.to_string()))
}

fn validate_external_authority_refs(
    refs: &CheckpointExternalAuthorityRefsV1,
) -> Result<(), CheckpointError> {
    for (field, digest) in [
        (
            "mission_heads_index_digest",
            &refs.mission_heads_index_digest,
        ),
        ("authority_wal_root_digest", &refs.authority_wal_root_digest),
        (
            "intent_core_store_root_digest",
            &refs.intent_core_store_root_digest,
        ),
        (
            "sentinel_outbox_watermark_digest",
            &refs.sentinel_outbox_watermark_digest,
        ),
        (
            "autonomy_epoch_record_digest",
            &refs.autonomy_epoch_record_digest,
        ),
    ] {
        validate_digest(field, digest)?;
    }
    Ok(())
}

fn validate_authority_receipt(
    receipt: &CheckpointAuthorityValidationReceiptV1,
    manifest: &CheckpointManifestV1,
    refs_digest: &str,
) -> Result<(), CheckpointError> {
    if receipt.schema != CHECKPOINT_AUTHORITY_RECEIPT_SCHEMA
        || receipt.checkpoint_id != manifest.checkpoint_id
        || receipt.external_authority_refs_digest != refs_digest
    {
        return Err(CheckpointError::AuthorityValidation(
            "validator receipt binding mismatch".to_string(),
        ));
    }
    validate_identifier("validator_id", &receipt.validator_id)?;
    if receipt.verified_at_unix_ms == 0 {
        return Err(CheckpointError::AuthorityValidation(
            "validator receipt verified_at_unix_ms is zero".to_string(),
        ));
    }
    validate_digest("protected_root_digest", &receipt.protected_root_digest)?;
    validate_digest("authority_receipt_digest", &receipt.receipt_digest)?;
    if receipt.compute_digest()? != receipt.receipt_digest {
        return Err(CheckpointError::AuthorityValidation(
            "validator receipt digest mismatch".to_string(),
        ));
    }
    Ok(())
}

fn write_new_file(
    path: &Path,
    bytes: &[u8],
    write_point: CheckpointFaultPoint,
    fsync_point: CheckpointFaultPoint,
    faults: &mut FaultCursor<'_>,
) -> Result<(), CheckpointError> {
    faults.check(write_point, path)?;
    let mut file = open_create_new_no_follow(path)?;
    file.write_all(bytes)?;
    faults.check(fsync_point, path)?;
    file.sync_all()?;
    Ok(())
}

fn read_regular_file_no_follow(
    path: &Path,
    max_bytes: Option<u64>,
) -> Result<Vec<u8>, CheckpointError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
        return Err(CheckpointError::SymlinkRefused(path.to_path_buf()));
    }
    if max_bytes.is_some_and(|limit| metadata.len() > limit) {
        return Err(CheckpointError::refused(
            "checkpoint_file_too_large",
            path.display().to_string(),
        ));
    }
    let mut file = open_read_no_follow(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn hash_regular_file_no_follow(path: &Path) -> Result<(String, u64), CheckpointError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
        return Err(CheckpointError::SymlinkRefused(path.to_path_buf()));
    }
    let file = open_read_no_follow(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total = total.saturating_add(read as u64);
    }
    Ok((hex_lower(&hasher.finalize()), total))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), CheckpointError> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(CheckpointError::refused(
            "checkpoint_invalid_identifier",
            format!("{field} must contain 1..=256 non-control characters"),
        ));
    }
    Ok(())
}

fn validate_digest(field: &'static str, value: &str) -> Result<(), CheckpointError> {
    if !is_digest(value) {
        return Err(CheckpointError::refused(
            "checkpoint_invalid_digest",
            format!("{field} must be 64 lowercase hexadecimal characters"),
        ));
    }
    Ok(())
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_relative_logical_path(value: &str) -> Result<(), CheckpointError> {
    if value.is_empty() || value.len() > 1024 {
        return Err(CheckpointError::refused(
            "checkpoint_invalid_relative_path",
            value,
        ));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(CheckpointError::refused(
            "checkpoint_path_traversal_refused",
            value,
        ));
    }
    Ok(())
}

fn normalize_checkpoint_root(root: &Path) -> Result<(PathBuf, PathBuf), CheckpointError> {
    let absolute = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    let root_name = absolute.file_name().ok_or_else(|| {
        CheckpointError::refused(
            "checkpoint_root_has_no_parent_namespace",
            absolute.display().to_string(),
        )
    })?;
    let supplied_parent = absolute.parent().ok_or_else(|| {
        CheckpointError::refused(
            "checkpoint_root_has_no_parent_namespace",
            absolute.display().to_string(),
        )
    })?;
    refuse_symlink_if_present(supplied_parent)?;
    fs::create_dir_all(supplied_parent)?;
    require_directory(supplied_parent)?;
    let parent = fs::canonicalize(supplied_parent)?;
    Ok((parent.clone(), parent.join(root_name)))
}

fn namespace_lease_name(root: &Path) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        format!(
            ".m1nd-checkpoint-namespace-{}.lease",
            sha256_bytes(root.as_os_str().as_bytes())
        )
    }
    #[cfg(not(unix))]
    {
        let encoded = root.to_string_lossy();
        format!(
            ".m1nd-checkpoint-namespace-{}.lease",
            sha256_bytes(encoded.as_bytes())
        )
    }
}

fn verify_directory_binding(
    code: &'static str,
    path: &Path,
    expected: DirectoryIdentity,
) -> Result<(), CheckpointError> {
    let observed = match directory_identity_at_path(path) {
        Ok(identity) => identity,
        Err(CheckpointError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            return Err(CheckpointError::refused(
                code,
                format!("bound directory disappeared: {}", path.display()),
            ));
        }
        Err(error) => return Err(error),
    };
    if observed != expected {
        return Err(CheckpointError::refused(
            code,
            format!(
                "directory identity for {} changed from {:?} to {:?}",
                path.display(),
                expected,
                observed
            ),
        ));
    }
    Ok(())
}

fn directory_identity_at_path(path: &Path) -> Result<DirectoryIdentity, CheckpointError> {
    #[cfg(windows)]
    {
        directory_identity_from_file(&open_directory_no_follow(path)?)
    }
    #[cfg(not(windows))]
    {
        let metadata = fs::symlink_metadata(path)?;
        if metadata_is_link_or_reparse(&metadata) {
            return Err(CheckpointError::SymlinkRefused(path.to_path_buf()));
        }
        if !metadata.is_dir() {
            return Err(CheckpointError::refused(
                "checkpoint_directory_identity_not_directory",
                path.display().to_string(),
            ));
        }
        directory_identity_from_metadata(&metadata)
    }
}

fn directory_identity_from_file(file: &File) -> Result<DirectoryIdentity, CheckpointError> {
    let metadata = file.metadata()?;
    if !metadata.is_dir() {
        return Err(CheckpointError::refused(
            "checkpoint_directory_descriptor_not_directory",
            "directory descriptor no longer reports a directory",
        ));
    }
    #[cfg(windows)]
    {
        let (device, inode) = crate::windows_durable_fs::directory_identity(file)?;
        Ok(DirectoryIdentity { device, inode })
    }
    #[cfg(not(windows))]
    directory_identity_from_metadata(&metadata)
}

#[cfg(unix)]
fn directory_identity_from_metadata(
    metadata: &fs::Metadata,
) -> Result<DirectoryIdentity, CheckpointError> {
    use std::os::unix::fs::MetadataExt;
    Ok(DirectoryIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(all(not(unix), not(windows)))]
fn directory_identity_from_metadata(
    _metadata: &fs::Metadata,
) -> Result<DirectoryIdentity, CheckpointError> {
    Err(CheckpointError::PlatformNotProven(
        "directory dev/inode identity requires a reviewed Windows primitive",
    ))
}

fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
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

fn refuse_symlink_if_present(path: &Path) -> Result<(), CheckpointError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata_is_link_or_reparse(&metadata) => {
            Err(CheckpointError::SymlinkRefused(path.to_path_buf()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn cleanup_unpublished_temporaries(root: &Path, checkpoints: &Path) -> Result<(), CheckpointError> {
    let mut removed_checkpoint_temporary = false;
    for entry in fs::read_dir(checkpoints)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let is_gc_tombstone = name.starts_with(".gc-");
        if !name.starts_with(".staging-") && !is_gc_tombstone {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
            return Err(CheckpointError::SymlinkRefused(path));
        }
        if is_gc_tombstone {
            // Canonical namespace retirement was already committed by the
            // write-through move. Antivirus, backup, or stale reader handles
            // may temporarily prevent physical reclamation on Windows; retry
            // next open without making the live store unavailable.
            let _ = fs::remove_dir_all(path);
        } else {
            fs::remove_dir_all(path)?;
            removed_checkpoint_temporary = true;
        }
    }
    if removed_checkpoint_temporary {
        #[cfg(unix)]
        sync_directory(checkpoints)?;
    }

    let mut removed_current_temporary = false;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !name.starts_with(".CURRENT.tmp-") {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
            return Err(CheckpointError::SymlinkRefused(path));
        }
        fs::remove_file(path)?;
        removed_current_temporary = true;
    }
    if removed_current_temporary {
        #[cfg(unix)]
        sync_directory(root)?;
    }
    Ok(())
}

#[cfg(unix)]
fn retire_checkpoint_directory(
    path: &Path,
    _checkpoints: &Path,
    _checkpoint_id: &str,
) -> Result<(), CheckpointError> {
    fs::remove_dir_all(path)?;
    Ok(())
}

#[cfg(windows)]
fn retire_checkpoint_directory(
    path: &Path,
    checkpoints: &Path,
    checkpoint_id: &str,
) -> Result<(), CheckpointError> {
    // Namespace removal is the durable GC commit on Windows. Physical space
    // reclamation is best-effort after the write-through tombstone move and is
    // retried by startup cleanup; receipts claim only the canonical id vanished.
    let nonce = TEMP_NONCE.fetch_add(1, Ordering::Relaxed);
    let tombstone = checkpoints.join(format!(
        ".gc-{checkpoint_id}-{}-{nonce}",
        std::process::id()
    ));
    crate::windows_durable_fs::move_new_write_through(path, &tombstone)?;
    let _ = fs::remove_dir_all(tombstone);
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn retire_checkpoint_directory(
    _path: &Path,
    _checkpoints: &Path,
    _checkpoint_id: &str,
) -> Result<(), CheckpointError> {
    Err(CheckpointError::PlatformNotProven(
        "durable checkpoint retirement requires a reviewed platform primitive",
    ))
}

fn require_directory(path: &Path) -> Result<(), CheckpointError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata_is_link_or_reparse(&metadata) {
        return Err(CheckpointError::SymlinkRefused(path.to_path_buf()));
    }
    if !metadata.is_dir() {
        return Err(CheckpointError::refused(
            "checkpoint_root_not_directory",
            path.display().to_string(),
        ));
    }
    Ok(())
}

fn now_unix_ms() -> Result<u64, CheckpointError> {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| {
            CheckpointError::refused("checkpoint_clock_before_epoch", error.to_string())
        })?;
    Ok(duration.as_millis().try_into().unwrap_or(u64::MAX))
}

struct DirectoryCleanupGuard {
    path: PathBuf,
    armed: bool,
}

impl DirectoryCleanupGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for DirectoryCleanupGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

struct FileCleanupGuard {
    path: PathBuf,
    armed: bool,
}

impl FileCleanupGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for FileCleanupGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
fn open_create_new_no_follow(path: &Path) -> Result<File, CheckpointError> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?)
}

#[cfg(windows)]
fn open_create_new_no_follow(path: &Path) -> Result<File, CheckpointError> {
    Ok(crate::windows_durable_fs::open_create_new_no_follow(path)?)
}

#[cfg(all(not(unix), not(windows)))]
fn open_create_new_no_follow(_path: &Path) -> Result<File, CheckpointError> {
    Err(CheckpointError::PlatformNotProven(
        "O_NOFOLLOW/create_new checkpoint writes require a reviewed Windows primitive",
    ))
}

#[cfg(unix)]
fn open_read_no_follow(path: &Path) -> Result<File, CheckpointError> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?)
}

#[cfg(unix)]
fn open_directory_no_follow(path: &Path) -> Result<File, CheckpointError> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?)
}

#[cfg(windows)]
fn open_directory_no_follow(path: &Path) -> Result<File, CheckpointError> {
    Ok(crate::windows_durable_fs::open_directory_no_follow(path)?)
}

#[cfg(all(not(unix), not(windows)))]
fn open_directory_no_follow(_path: &Path) -> Result<File, CheckpointError> {
    Err(CheckpointError::PlatformNotProven(
        "directory-anchored checkpoint writer lease requires a reviewed Windows primitive",
    ))
}

#[cfg(windows)]
fn open_read_no_follow(path: &Path) -> Result<File, CheckpointError> {
    Ok(crate::windows_durable_fs::open_read_no_follow(path)?)
}

#[cfg(all(not(unix), not(windows)))]
fn open_read_no_follow(_path: &Path) -> Result<File, CheckpointError> {
    Err(CheckpointError::PlatformNotProven(
        "no-follow checkpoint reads require a reviewed Windows primitive",
    ))
}

#[cfg(unix)]
fn open_append_no_follow(path: &Path) -> Result<File, CheckpointError> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?)
}

#[cfg(windows)]
fn open_append_no_follow(path: &Path) -> Result<File, CheckpointError> {
    Ok(crate::windows_durable_fs::open_lock_file_no_follow(path)?)
}

#[cfg(all(not(unix), not(windows)))]
fn open_append_no_follow(_path: &Path) -> Result<File, CheckpointError> {
    Err(CheckpointError::PlatformNotProven(
        "single-writer checkpoint lock requires a reviewed Windows primitive",
    ))
}

#[cfg(unix)]
fn lock_file_exclusive(file: &File) -> Result<(), CheckpointError> {
    use std::os::fd::AsRawFd;
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        Ok(())
    } else {
        Err(CheckpointError::WriterLocked(
            io::Error::last_os_error().to_string(),
        ))
    }
}

#[cfg(windows)]
fn lock_file_exclusive(file: &File) -> Result<(), CheckpointError> {
    crate::windows_durable_fs::lock_file_exclusive(file, true)
        .map_err(|error| CheckpointError::WriterLocked(error.to_string()))
}

#[cfg(all(not(unix), not(windows)))]
fn lock_file_exclusive(_file: &File) -> Result<(), CheckpointError> {
    Err(CheckpointError::PlatformNotProven(
        "single-writer checkpoint lock requires a reviewed Windows primitive",
    ))
}

#[cfg(unix)]
fn unlock_file(file: &File) {
    use std::os::fd::AsRawFd;
    let _ = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
}

#[cfg(windows)]
fn unlock_file(file: &File) {
    let _ = crate::windows_durable_fs::unlock_file(file);
}

#[cfg(all(not(unix), not(windows)))]
fn unlock_file(_file: &File) {}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), CheckpointError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn publish_new_path(source: &Path, destination: &Path) -> Result<(), CheckpointError> {
    fs::rename(source, destination)?;
    Ok(())
}

#[cfg(windows)]
fn publish_new_path(source: &Path, destination: &Path) -> Result<(), CheckpointError> {
    crate::windows_durable_fs::move_new_write_through(source, destination)?;
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn publish_new_path(_source: &Path, _destination: &Path) -> Result<(), CheckpointError> {
    Err(CheckpointError::PlatformNotProven(
        "durable checkpoint publication requires a reviewed platform primitive",
    ))
}

#[cfg(unix)]
fn replace_path(source: &Path, destination: &Path) -> Result<(), CheckpointError> {
    fs::rename(source, destination)?;
    Ok(())
}

#[cfg(windows)]
fn replace_path(source: &Path, destination: &Path) -> Result<(), CheckpointError> {
    crate::windows_durable_fs::replace_write_through(source, destination)?;
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn replace_path(_source: &Path, _destination: &Path) -> Result<(), CheckpointError> {
    Err(CheckpointError::PlatformNotProven(
        "durable CURRENT replacement requires a reviewed platform primitive",
    ))
}
