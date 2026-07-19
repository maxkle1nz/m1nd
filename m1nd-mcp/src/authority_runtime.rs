//! Owner-side G2 authority runtime foundation.
//!
//! The runtime is deliberately fail closed. It owns one serial mutex, one
//! durable replay ledger, one append-only hash-chained journal, and one
//! authoritative state record pinned by an injected protected-epoch backend.
//! The included software backend reports `SOFTWARE_TEST_ONLY_NOT_PROVEN`; it is
//! useful for deterministic batteries but makes no hardware anti-rollback
//! claim. Cross-artifact publication uses an fsynced `PREPARED` descriptor:
//! before the protected CAS recovery removes only exact descriptor-bound tails;
//! after that CAS it forward-completes only the exact bound next state. This is
//! an old-or-new recovery protocol, not a claim that multiple files and a
//! hardware epoch update are physically atomic. Transport and production-key
//! wiring are intentionally out of scope.

#[cfg(unix)]
use std::collections::HashMap;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
#[cfg(unix)]
use std::sync::{OnceLock, Weak};

use m1nd_control::autonomy::SafetyState;
use m1nd_control::{
    canonical_json, canonical_json_string, digest_canonical, m1nd10_action_catalog,
    verify_capability, verify_capability_once, ActionCatalogEntryV1, ActionCatalogError,
    ActionCatalogV1, ActionId, ActionPolicyRegistryV1, ActiveMode, AuthorityCapabilityV1,
    AuthorityCryptoError, AuthorityFloor, AuthorityVariant, AutonomyTier, CanonicalError,
    CapabilityKind, CapabilityVerificationContext, Effect, Ingress, OpaqueSignature, PolicyError,
    ReachablePolicyTupleV1, ReplayClaimV1, ReplayDurability, ReplayLedger, ReplayLedgerError,
    ReplayReceiptV1, RiskClass, Role, VerificationKeyRegistryV1, AUTHORITY_CAPABILITY_SCHEMA,
    DEFAULT_AUTHORITY_CLOCK_SKEW_MS, REPLAY_CLAIM_SCHEMA, REPLAY_LEDGER_RECORD_DIGEST_DOMAIN,
    REPLAY_LEDGER_RECORD_SCHEMA,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::autonomy_manifest::{
    validate_autonomy_authority_binding, AutonomyAdmissionOutcomeV1, AutonomyAdmissionOwner,
    AutonomyAuthorityEvidenceV1, AutonomyManifestProjectionV1, ExpectedAutonomyAuthorityBindingV1,
};

pub const AUTHORITY_RUNTIME_STATE_SCHEMA: &str = "m1nd-authority-runtime-state-v1";
pub const AUTHORITY_RUNTIME_STATE_DIGEST_DOMAIN: &str = "m1nd-authority-runtime-state-v1";
pub const AUTHORITY_JOURNAL_RECORD_SCHEMA: &str = "m1nd-authority-runtime-journal-v1";
pub const AUTHORITY_JOURNAL_RECORD_DIGEST_DOMAIN: &str = "m1nd-authority-runtime-journal-v1";
pub const SESSION_CHALLENGE_SCHEMA: &str = "m1nd-authority-session-challenge-v1";
pub const SESSION_CHALLENGE_DIGEST_DOMAIN: &str = "m1nd-authority-session-challenge-v1";
pub const SESSION_ID_DIGEST_DOMAIN: &str = "m1nd-authority-session-id-v1";
pub const SAFETY_ACTUATOR_ATTEMPT_SCHEMA: &str = "m1nd-safety-actuator-attempt-v1";
pub const SAFETY_ACTUATOR_ATTEMPT_DIGEST_DOMAIN: &str = "m1nd-safety-actuator-attempt-v1";
pub const AUTHORIZATION_RECEIPT_DIGEST_DOMAIN: &str = "m1nd-runtime-authorization-receipt-v1";
pub const AUTHORIZATION_RECEIPT_SCHEMA: &str = "m1nd-runtime-authorization-receipt-v1";
pub const AUTHORIZATION_RECEIPT_SIGNATURE_DOMAIN: &str =
    "m1nd-runtime-authorization-receipt-signature-v1";
pub const SERVICE_IDENTITY_ASSERTION_SCHEMA: &str = "m1nd-service-identity-assertion-v1";
pub const SERVICE_IDENTITY_ASSERTION_DIGEST_DOMAIN: &str = "m1nd-service-identity-assertion-v1";
pub const AUTHORITY_TRANSITION_DESCRIPTOR_SCHEMA: &str = "m1nd-authority-transition-descriptor-v1";
pub const AUTHORITY_TRANSITION_DESCRIPTOR_DIGEST_DOMAIN: &str =
    "m1nd-authority-transition-descriptor-v1";

const STATE_FILE_NAME: &str = "authority-state.json";
const JOURNAL_FILE_NAME: &str = "authority-journal.jsonl";
const REPLAY_FILE_NAME: &str = "authority-replay.jsonl";
#[cfg(unix)]
const LOCK_FILE_NAME: &str = "authority-owner.lock";
const TRANSITION_DESCRIPTOR_FILE_NAME: &str = "authority-transition.prepared.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProtectedEpochAssurance {
    SoftwareTestOnlyNotProven,
    HardwareProtectedAttested,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedEpochSnapshotV1 {
    pub epoch: u64,
    pub record_digest: String,
}

/// The backend is a narrow anti-rollback root, not a general state store.
/// Implementations must atomically compare the prior snapshot and publish the
/// next snapshot or return an error without claiming success.
pub trait ProtectedEpochBackend: Send {
    fn assurance(&self) -> ProtectedEpochAssurance;
    fn read_latest(&self) -> Result<Option<ProtectedEpochSnapshotV1>, String>;
    fn compare_and_advance(
        &mut self,
        expected: Option<&ProtectedEpochSnapshotV1>,
        next: &ProtectedEpochSnapshotV1,
    ) -> Result<(), String>;
}

/// Shared deterministic backend for tests and development batteries.
/// Its name and assurance intentionally state that it is not anti-rollback
/// proof and must never be projected as hardware protected.
#[derive(Clone, Default)]
pub struct SoftwareTestProtectedEpochBackend {
    state: Arc<Mutex<Option<ProtectedEpochSnapshotV1>>>,
}

impl SoftwareTestProtectedEpochBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Option<ProtectedEpochSnapshotV1> {
        self.state.lock().clone()
    }

    #[cfg(all(test, unix))]
    fn force_snapshot(&self, snapshot: Option<ProtectedEpochSnapshotV1>) {
        *self.state.lock() = snapshot;
    }
}

impl ProtectedEpochBackend for SoftwareTestProtectedEpochBackend {
    fn assurance(&self) -> ProtectedEpochAssurance {
        ProtectedEpochAssurance::SoftwareTestOnlyNotProven
    }

    fn read_latest(&self) -> Result<Option<ProtectedEpochSnapshotV1>, String> {
        Ok(self.state.lock().clone())
    }

    fn compare_and_advance(
        &mut self,
        expected: Option<&ProtectedEpochSnapshotV1>,
        next: &ProtectedEpochSnapshotV1,
    ) -> Result<(), String> {
        let mut state = self.state.lock();
        if state.as_ref() != expected {
            return Err("software protected epoch compare-and-swap mismatch".to_string());
        }
        let expected_epoch = expected.map_or(0, |snapshot| snapshot.epoch);
        if next.epoch != expected_epoch.saturating_add(1) {
            return Err("software protected epoch must advance exactly once".to_string());
        }
        *state = Some(next.clone());
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct AuthorityRuntimeConfig {
    pub root: PathBuf,
    pub organism_id: String,
    pub repo_id: String,
    pub brain_id: String,
    pub audience: String,
    pub constitution_digest: String,
    pub constitution_epoch: u64,
    pub grants_digest: String,
    pub policy_registry_digest: String,
    /// Exact policy tuples accepted by this owner. A digest alone is not an
    /// authorization policy: every authorization must find one byte-exact
    /// reachable tuple and rule here.
    pub policy_registry: ActionPolicyRegistryV1,
    pub service_identities: BTreeMap<String, PinnedServiceIdentityV1>,
    pub safety_kernel_digest: String,
    pub safety_actuator_identity_key_binary_policy_digest: String,
    pub max_future_clock_skew_ms: u64,
}

impl AuthorityRuntimeConfig {
    pub fn with_default_clock_skew(mut self) -> Self {
        self.max_future_clock_skew_ms = DEFAULT_AUTHORITY_CLOCK_SKEW_MS;
        self
    }

    fn validate(&self) -> Result<(), AuthorityRuntimeError> {
        for (field, value) in [
            ("organism_id", self.organism_id.as_str()),
            ("repo_id", self.repo_id.as_str()),
            ("brain_id", self.brain_id.as_str()),
            ("audience", self.audience.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        for (field, digest) in [
            ("constitution_digest", self.constitution_digest.as_str()),
            ("grants_digest", self.grants_digest.as_str()),
            (
                "policy_registry_digest",
                self.policy_registry_digest.as_str(),
            ),
            ("safety_kernel_digest", self.safety_kernel_digest.as_str()),
            (
                "safety_actuator_identity_key_binary_policy_digest",
                self.safety_actuator_identity_key_binary_policy_digest
                    .as_str(),
            ),
        ] {
            require_digest(field, digest)?;
        }
        self.policy_registry.validate()?;
        if self.policy_registry.policy_digest != self.policy_registry_digest {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "policy_registry_digest",
            });
        }
        for (service_id, identity) in &self.service_identities {
            if service_id != &identity.service_id
                || identity.organism_id != self.organism_id
                || identity.brain_id != self.brain_id
                || identity.audience != self.audience
                || identity.allowed_actions.is_empty()
            {
                return Err(AuthorityRuntimeError::InvalidContract {
                    detail: format!("invalid pinned service identity '{service_id}'"),
                });
            }
            for (field, value) in [
                ("service.subject_id", identity.subject_id.as_str()),
                ("service.key_id", identity.key_id.as_str()),
            ] {
                require_non_empty(field, value)?;
            }
            require_digest(
                "service.identity_key_binary_policy_digest",
                &identity.identity_key_binary_policy_digest,
            )?;
        }
        if self.root.as_os_str().is_empty() {
            return Err(AuthorityRuntimeError::InvalidContract {
                detail: "authority runtime root is empty".to_string(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityRuntimeStateCoreV1 {
    pub organism_id: String,
    pub repo_id: String,
    pub brain_id: String,
    pub audience: String,
    pub revision: u64,
    pub active_mode: ActiveMode,
    pub activation_receipt_id: Option<String>,
    pub constitution_digest: String,
    pub constitution_epoch: u64,
    pub autonomy_epoch: u64,
    pub grants_digest: String,
    pub policy_registry_digest: String,
    pub action_catalog_digest: String,
    pub safety_kernel_digest: String,
    pub safety_actuator_identity_key_binary_policy_digest: String,
    pub issuance_frozen: bool,
    pub safety_state: SafetyState,
    pub protected_epoch: u64,
    pub journal_sequence: u64,
    pub journal_root_digest: String,
    pub replay_sequence: u64,
    pub replay_root_digest: Option<String>,
    pub updated_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityRuntimeStateV1 {
    pub schema: String,
    pub core: AuthorityRuntimeStateCoreV1,
    pub record_digest: String,
}

impl AuthorityRuntimeStateV1 {
    fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(AUTHORITY_RUNTIME_STATE_DIGEST_DOMAIN, &self.core)
    }

    fn seal(&mut self) -> Result<(), CanonicalError> {
        self.record_digest = self.compute_digest()?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn new_for_broker_test(core: AuthorityRuntimeStateCoreV1) -> Self {
        let mut state = Self {
            schema: AUTHORITY_RUNTIME_STATE_SCHEMA.to_string(),
            core,
            record_digest: String::new(),
        };
        state.seal().expect("test authority state must seal");
        state
    }

    fn protected_snapshot(&self) -> ProtectedEpochSnapshotV1 {
        ProtectedEpochSnapshotV1 {
            epoch: self.core.protected_epoch,
            record_digest: self.record_digest.clone(),
        }
    }

    fn validate(
        &self,
        config: &AuthorityRuntimeConfig,
        catalog: &ActionCatalogV1,
    ) -> Result<(), AuthorityRuntimeError> {
        if self.schema != AUTHORITY_RUNTIME_STATE_SCHEMA {
            return Err(AuthorityRuntimeError::CorruptState {
                detail: format!("unsupported state schema '{}'", self.schema),
            });
        }
        if self.core.organism_id != config.organism_id
            || self.core.repo_id != config.repo_id
            || self.core.brain_id != config.brain_id
            || self.core.audience != config.audience
        {
            return Err(AuthorityRuntimeError::CorruptState {
                detail: "state owner identity does not match runtime configuration".to_string(),
            });
        }
        // Constitution/grant pins become a protected G9-owned dynamic mirror
        // after bootstrap. Their changes are accepted only through the
        // synchronized autonomy transition below and are still covered by the
        // G2 state digest, journal and protected epoch. Static binary/policy
        // and safety actuator pins remain configuration-owned on every open.
        if self.core.policy_registry_digest != config.policy_registry_digest
            || self.core.safety_kernel_digest != config.safety_kernel_digest
            || self.core.safety_actuator_identity_key_binary_policy_digest
                != config.safety_actuator_identity_key_binary_policy_digest
        {
            return Err(AuthorityRuntimeError::CorruptState {
                detail: "state governance pins do not match runtime configuration".to_string(),
            });
        }
        for (field, digest) in [
            ("state.record_digest", self.record_digest.as_str()),
            (
                "state.constitution_digest",
                self.core.constitution_digest.as_str(),
            ),
            ("state.grants_digest", self.core.grants_digest.as_str()),
            (
                "state.policy_registry_digest",
                self.core.policy_registry_digest.as_str(),
            ),
            (
                "state.action_catalog_digest",
                self.core.action_catalog_digest.as_str(),
            ),
            (
                "state.safety_kernel_digest",
                self.core.safety_kernel_digest.as_str(),
            ),
            (
                "state.safety_actuator_identity_key_binary_policy_digest",
                self.core
                    .safety_actuator_identity_key_binary_policy_digest
                    .as_str(),
            ),
            (
                "state.journal_root_digest",
                self.core.journal_root_digest.as_str(),
            ),
        ] {
            require_digest(field, digest)?;
        }
        let computed = self.compute_digest()?;
        if computed != self.record_digest {
            return Err(AuthorityRuntimeError::CorruptState {
                detail: "authority state self-digest mismatch".to_string(),
            });
        }
        if self.core.action_catalog_digest != catalog.catalog_digest {
            return Err(AuthorityRuntimeError::CorruptState {
                detail: "authority state action catalog digest is stale".to_string(),
            });
        }
        if self.core.revision == 0
            && (self.core.active_mode != ActiveMode::HumanGated
                || self.core.activation_receipt_id.is_some()
                || self.core.constitution_digest != config.constitution_digest
                || self.core.constitution_epoch != config.constitution_epoch
                || self.core.grants_digest != config.grants_digest
                || !self.core.issuance_frozen
                || self.core.safety_state != SafetyState::Frozen)
        {
            return Err(AuthorityRuntimeError::ActivationConflict {
                detail: "bootstrap must be frozen HUMAN_GATED with no activation receipt"
                    .to_string(),
            });
        }
        if self.core.active_mode != ActiveMode::HumanGated
            && self
                .core
                .activation_receipt_id
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err(AuthorityRuntimeError::ActivationConflict {
                detail: "autonomous mode has no activation receipt".to_string(),
            });
        }
        match self.core.safety_state {
            SafetyState::Healthy if self.core.issuance_frozen => {
                return Err(AuthorityRuntimeError::ActivationConflict {
                    detail: "HEALTHY state cannot have issuance frozen".to_string(),
                });
            }
            SafetyState::Frozen | SafetyState::PendingRed | SafetyState::Recovering
                if !self.core.issuance_frozen =>
            {
                return Err(AuthorityRuntimeError::ActivationConflict {
                    detail: "non-HEALTHY safety state must freeze issuance".to_string(),
                });
            }
            _ => {}
        }
        if self.core.protected_epoch == 0 || self.core.journal_sequence == 0 {
            return Err(AuthorityRuntimeError::CorruptState {
                detail: "protected epoch and journal sequence start at one".to_string(),
            });
        }
        match (
            self.core.replay_sequence,
            self.core.replay_root_digest.as_deref(),
        ) {
            (0, None) => {}
            (0, Some(_)) | (_, None) => {
                return Err(AuthorityRuntimeError::CorruptState {
                    detail: "replay sequence/root presence mismatch".to_string(),
                });
            }
            (_, Some(root)) => require_digest("state.replay_root_digest", root)?,
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorityJournalEventKind {
    BootstrapFrozen,
    BootstrapVerified,
    SessionAuthenticated,
    AutonomyAuthoritySynchronized,
    PositiveMutationAuthorized,
    SafetyMutationAuthorized,
    AutonomyMirrorMismatchFrozen,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityJournalRecordCoreV1 {
    pub sequence: u64,
    pub event_kind: AuthorityJournalEventKind,
    pub payload_digest: String,
    pub protected_epoch: u64,
    pub previous_record_digest: Option<String>,
    pub recorded_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityJournalRecordV1 {
    pub schema: String,
    pub core: AuthorityJournalRecordCoreV1,
    pub record_digest: String,
}

impl AuthorityJournalRecordV1 {
    fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(AUTHORITY_JOURNAL_RECORD_DIGEST_DOMAIN, &self.core)
    }

    fn seal(&mut self) -> Result<(), CanonicalError> {
        self.record_digest = self.compute_digest()?;
        Ok(())
    }

    fn validate(
        &self,
        expected_sequence: u64,
        expected_previous: Option<&str>,
    ) -> Result<(), AuthorityRuntimeError> {
        if self.schema != AUTHORITY_JOURNAL_RECORD_SCHEMA
            || self.core.sequence != expected_sequence
            || self.core.previous_record_digest.as_deref() != expected_previous
            || self.core.protected_epoch != expected_sequence
        {
            return Err(AuthorityRuntimeError::CorruptJournal {
                detail: format!("journal chain mismatch at sequence {expected_sequence}"),
            });
        }
        require_digest("journal.payload_digest", &self.core.payload_digest)?;
        require_digest("journal.record_digest", &self.record_digest)?;
        let computed = self.compute_digest()?;
        if computed != self.record_digest {
            return Err(AuthorityRuntimeError::CorruptJournal {
                detail: format!("journal self-digest mismatch at sequence {expected_sequence}"),
            });
        }
        Ok(())
    }

    fn encoded_line(&self) -> Result<Vec<u8>, AuthorityRuntimeError> {
        let mut encoded = canonical_json_string(self)?.into_bytes();
        encoded.push(b'\n');
        Ok(encoded)
    }
}

struct AuthorityJournal {
    path: PathBuf,
    file: File,
    sequence: u64,
    tail_digest: Option<String>,
    known_len: u64,
    poisoned: bool,
}

impl AuthorityJournal {
    fn create(path: PathBuf) -> Result<Self, AuthorityRuntimeError> {
        refuse_symlink(&path)?;
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .append(true)
            .open(&path)
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "create_authority_journal",
                source,
            })?;
        file.sync_all()
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "sync_new_authority_journal",
                source,
            })?;
        sync_parent(&path)?;
        Ok(Self {
            path,
            file,
            sequence: 0,
            tail_digest: None,
            known_len: 0,
            poisoned: false,
        })
    }

    fn open(path: PathBuf) -> Result<Self, AuthorityRuntimeError> {
        refuse_symlink(&path)?;
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .open(&path)
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "open_authority_journal",
                source,
            })?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "read_authority_journal",
                source,
            })?;
        let (sequence, tail_digest) = replay_journal(&bytes)?;
        Ok(Self {
            path,
            file,
            sequence,
            tail_digest,
            known_len: bytes.len() as u64,
            poisoned: false,
        })
    }

    fn prepare(
        &mut self,
        event_kind: AuthorityJournalEventKind,
        payload_digest: String,
        protected_epoch: u64,
        recorded_at: u64,
    ) -> Result<AuthorityJournalRecordV1, AuthorityRuntimeError> {
        if self.poisoned {
            return Err(AuthorityRuntimeError::Poisoned);
        }
        let observed_len = self
            .file
            .metadata()
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "journal_length_before_append",
                source,
            })?
            .len();
        if observed_len != self.known_len {
            self.poisoned = true;
            return Err(AuthorityRuntimeError::ConcurrentModification {
                detail: "authority journal length changed outside owner serial".to_string(),
            });
        }
        let next_sequence = self.sequence.saturating_add(1);
        if protected_epoch != next_sequence {
            return Err(AuthorityRuntimeError::InvalidContract {
                detail: "journal/protected epoch must advance in one shared sequence".to_string(),
            });
        }
        let mut record = AuthorityJournalRecordV1 {
            schema: AUTHORITY_JOURNAL_RECORD_SCHEMA.to_string(),
            core: AuthorityJournalRecordCoreV1 {
                sequence: next_sequence,
                event_kind,
                payload_digest,
                protected_epoch,
                previous_record_digest: self.tail_digest.clone(),
                recorded_at,
            },
            record_digest: String::new(),
        };
        record.seal()?;
        Ok(record)
    }

    fn append_prepared(
        &mut self,
        record: &AuthorityJournalRecordV1,
    ) -> Result<(), AuthorityRuntimeError> {
        if self.poisoned {
            return Err(AuthorityRuntimeError::Poisoned);
        }
        let expected_sequence = self.sequence.saturating_add(1);
        record.validate(expected_sequence, self.tail_digest.as_deref())?;
        if record.core.protected_epoch != expected_sequence {
            return Err(AuthorityRuntimeError::InvalidContract {
                detail: "prepared journal/protected epoch mismatch".to_string(),
            });
        }
        let observed_len = self
            .file
            .metadata()
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "journal_length_before_prepared_append",
                source,
            })?
            .len();
        if observed_len != self.known_len {
            self.poisoned = true;
            return Err(AuthorityRuntimeError::ConcurrentModification {
                detail: "authority journal length changed outside owner serial".to_string(),
            });
        }
        let encoded = record.encoded_line()?;
        if let Err(source) = self.file.write_all(&encoded) {
            self.poisoned = true;
            return Err(AuthorityRuntimeError::Io {
                operation: "append_authority_journal",
                source,
            });
        }
        if let Err(source) = self.file.sync_all() {
            self.poisoned = true;
            return Err(AuthorityRuntimeError::Io {
                operation: "sync_authority_journal",
                source,
            });
        }
        self.known_len = self.known_len.saturating_add(encoded.len() as u64);
        self.sequence = expected_sequence;
        self.tail_digest = Some(record.record_digest.clone());
        Ok(())
    }
}

fn replay_journal(bytes: &[u8]) -> Result<(u64, Option<String>), AuthorityRuntimeError> {
    if bytes.is_empty() {
        return Ok((0, None));
    }
    if !bytes.ends_with(b"\n") {
        return Err(AuthorityRuntimeError::CorruptJournal {
            detail: "journal has a torn or unterminated tail".to_string(),
        });
    }
    let mut sequence = 0u64;
    let mut tail: Option<String> = None;
    for (index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        let record: AuthorityJournalRecordV1 = serde_json::from_slice(line).map_err(|error| {
            AuthorityRuntimeError::CorruptJournal {
                detail: format!("invalid journal JSON at line {}: {error}", index + 1),
            }
        })?;
        let expected = sequence.saturating_add(1);
        record.validate(expected, tail.as_deref())?;
        sequence = expected;
        tail = Some(record.record_digest);
    }
    Ok((sequence, tail))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorityReplayRecordV1 {
    schema: String,
    sequence: u64,
    previous_record_digest: Option<String>,
    claim: ReplayClaimV1,
    claim_digest: String,
    scope_digest: String,
    record_digest: String,
}

#[derive(Serialize)]
struct AuthorityReplayRecordMaterialV1<'a> {
    schema: &'a str,
    sequence: u64,
    previous_record_digest: Option<&'a str>,
    claim: &'a ReplayClaimV1,
    claim_digest: &'a str,
    scope_digest: &'a str,
}

impl AuthorityReplayRecordV1 {
    fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(
            REPLAY_LEDGER_RECORD_DIGEST_DOMAIN,
            &AuthorityReplayRecordMaterialV1 {
                schema: &self.schema,
                sequence: self.sequence,
                previous_record_digest: self.previous_record_digest.as_deref(),
                claim: &self.claim,
                claim_digest: &self.claim_digest,
                scope_digest: &self.scope_digest,
            },
        )
    }

    fn validate(
        &self,
        expected_sequence: u64,
        expected_previous: Option<&str>,
    ) -> Result<(), AuthorityRuntimeError> {
        if self.schema != REPLAY_LEDGER_RECORD_SCHEMA
            || self.sequence != expected_sequence
            || self.previous_record_digest.as_deref() != expected_previous
        {
            return Err(AuthorityRuntimeError::CorruptReplay {
                detail: format!("replay chain mismatch at sequence {expected_sequence}"),
            });
        }
        self.claim.validate_at(self.claim.issued_at, 0)?;
        if self.claim.claim_digest()? != self.claim_digest
            || self.claim.scope_digest()? != self.scope_digest
            || self.compute_digest()? != self.record_digest
        {
            return Err(AuthorityRuntimeError::CorruptReplay {
                detail: format!("replay digest mismatch at sequence {expected_sequence}"),
            });
        }
        Ok(())
    }

    fn encoded_line(&self) -> Result<Vec<u8>, AuthorityRuntimeError> {
        let mut encoded = canonical_json_string(self)?.into_bytes();
        encoded.push(b'\n');
        Ok(encoded)
    }
}

struct AuthorityReplayLedger {
    path: PathBuf,
    file: File,
    consumed_scopes: BTreeSet<String>,
    sequence: u64,
    tail_digest: Option<String>,
    known_len: u64,
    pending: Option<AuthorityReplayRecordV1>,
    poisoned: bool,
}

impl AuthorityReplayLedger {
    fn create(path: PathBuf) -> Result<Self, AuthorityRuntimeError> {
        refuse_symlink(&path)?;
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .append(true)
            .open(&path)
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "create_authority_replay",
                source,
            })?;
        file.sync_all()
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "sync_new_authority_replay",
                source,
            })?;
        sync_parent(&path)?;
        Ok(Self {
            path,
            file,
            consumed_scopes: BTreeSet::new(),
            sequence: 0,
            tail_digest: None,
            known_len: 0,
            pending: None,
            poisoned: false,
        })
    }

    fn open(path: PathBuf) -> Result<Self, AuthorityRuntimeError> {
        refuse_symlink(&path)?;
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .open(&path)
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "open_authority_replay",
                source,
            })?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "read_authority_replay",
                source,
            })?;
        let (consumed_scopes, sequence, tail_digest) = replay_authority_records(&bytes)?;
        Ok(Self {
            path,
            file,
            consumed_scopes,
            sequence,
            tail_digest,
            known_len: bytes.len() as u64,
            pending: None,
            poisoned: false,
        })
    }

    fn consumed_count(&self) -> usize {
        self.sequence as usize
    }

    fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    fn pending_record(&self) -> Option<&AuthorityReplayRecordV1> {
        self.pending.as_ref()
    }

    fn abort_pending(&mut self) {
        self.pending = None;
    }

    fn append_pending(&mut self) -> Result<(), AuthorityRuntimeError> {
        if self.poisoned {
            return Err(AuthorityRuntimeError::Poisoned);
        }
        let record =
            self.pending
                .clone()
                .ok_or_else(|| AuthorityRuntimeError::InvalidContract {
                    detail: "transaction has no staged replay record".to_string(),
                })?;
        record.validate(self.sequence.saturating_add(1), self.tail_digest.as_deref())?;
        let observed_len = self
            .file
            .metadata()
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "replay_length_before_prepared_append",
                source,
            })?
            .len();
        if observed_len != self.known_len {
            self.poisoned = true;
            return Err(AuthorityRuntimeError::ConcurrentModification {
                detail: "authority replay length changed outside owner serial".to_string(),
            });
        }
        let encoded = record.encoded_line()?;
        if let Err(source) = self.file.write_all(&encoded) {
            self.poisoned = true;
            return Err(AuthorityRuntimeError::Io {
                operation: "append_authority_replay",
                source,
            });
        }
        if let Err(source) = self.file.sync_all() {
            self.poisoned = true;
            return Err(AuthorityRuntimeError::Io {
                operation: "sync_authority_replay",
                source,
            });
        }
        self.known_len = self.known_len.saturating_add(encoded.len() as u64);
        self.sequence = record.sequence;
        self.tail_digest = Some(record.record_digest);
        self.consumed_scopes.insert(record.scope_digest);
        self.pending = None;
        Ok(())
    }
}

impl ReplayLedger for AuthorityReplayLedger {
    fn consume(
        &mut self,
        claim: &ReplayClaimV1,
        now_ms: u64,
        max_future_clock_skew_ms: u64,
    ) -> Result<ReplayReceiptV1, ReplayLedgerError> {
        if self.poisoned || self.pending.is_some() {
            return Err(ReplayLedgerError::Poisoned);
        }
        claim.validate_at(now_ms, max_future_clock_skew_ms)?;
        let claim_digest = claim.claim_digest()?;
        let scope_digest = claim.scope_digest()?;
        if self.consumed_scopes.contains(&scope_digest) {
            return Err(ReplayLedgerError::Replay { scope_digest });
        }
        let mut record = AuthorityReplayRecordV1 {
            schema: REPLAY_LEDGER_RECORD_SCHEMA.to_string(),
            sequence: self.sequence.saturating_add(1),
            previous_record_digest: self.tail_digest.clone(),
            claim: claim.clone(),
            claim_digest: claim_digest.clone(),
            scope_digest: scope_digest.clone(),
            record_digest: String::new(),
        };
        record.record_digest = record.compute_digest()?;
        let receipt = ReplayReceiptV1 {
            sequence: record.sequence,
            claim_digest,
            scope_digest,
            durability: ReplayDurability::Volatile,
        };
        self.pending = Some(record);
        Ok(receipt)
    }
}

fn replay_authority_records(
    bytes: &[u8],
) -> Result<(BTreeSet<String>, u64, Option<String>), AuthorityRuntimeError> {
    if bytes.is_empty() {
        return Ok((BTreeSet::new(), 0, None));
    }
    if !bytes.ends_with(b"\n") {
        return Err(AuthorityRuntimeError::CorruptReplay {
            detail: "replay ledger has a torn or unterminated tail".to_string(),
        });
    }
    let mut consumed = BTreeSet::new();
    let mut sequence = 0u64;
    let mut tail: Option<String> = None;
    for (index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        let record: AuthorityReplayRecordV1 =
            serde_json::from_slice(line).map_err(|error| AuthorityRuntimeError::CorruptReplay {
                detail: format!("invalid replay JSON at line {}: {error}", index + 1),
            })?;
        let expected = sequence.saturating_add(1);
        record.validate(expected, tail.as_deref())?;
        if !consumed.insert(record.scope_digest.clone()) {
            return Err(AuthorityRuntimeError::CorruptReplay {
                detail: format!("duplicate replay scope at sequence {expected}"),
            });
        }
        sequence = expected;
        tail = Some(record.record_digest);
    }
    Ok((consumed, sequence, tail))
}

#[cfg(unix)]
#[derive(Debug)]
struct OwnerLeaseToken {
    canonical_lock_path: PathBuf,
    canonical_root_path: PathBuf,
    root_directory: File,
    root_identity: AuthorityRootIdentity,
    file: File,
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AuthorityRootIdentity {
    device: u64,
    inode: u64,
}

#[cfg(not(unix))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AuthorityRootIdentity;

#[cfg(unix)]
impl Drop for OwnerLeaseToken {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
        if let Some(registry) = OWNER_LEASE_REGISTRY.get() {
            let mut registry = registry.lock();
            if registry
                .get(&self.canonical_lock_path)
                .and_then(Weak::upgrade)
                .is_none_or(|token| std::ptr::eq(Arc::as_ptr(&token), self))
            {
                registry.remove(&self.canonical_lock_path);
            }
        }
    }
}

#[cfg(unix)]
use std::os::fd::AsRawFd;

#[cfg(unix)]
static OWNER_LEASE_REGISTRY: OnceLock<Mutex<HashMap<PathBuf, Weak<OwnerLeaseToken>>>> =
    OnceLock::new();

#[cfg(unix)]
#[derive(Clone, Debug)]
struct OwnerLease {
    _token: Arc<OwnerLeaseToken>,
}

#[cfg(unix)]
impl OwnerLease {
    fn acquire(root: &Path) -> Result<Self, AuthorityRuntimeError> {
        // Refuse an already-present symlink before create_dir_all/canonicalize
        // can silently turn a caller-controlled alias into the authority root.
        refuse_symlink(root)?;
        fs::create_dir_all(root).map_err(|source| AuthorityRuntimeError::Io {
            operation: "create_authority_runtime_root",
            source,
        })?;
        let canonical_root =
            fs::canonicalize(root).map_err(|source| AuthorityRuntimeError::Io {
                operation: "canonicalize_authority_runtime_root",
                source,
            })?;
        let root_directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&canonical_root)
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "open_authority_runtime_root",
                source,
            })?;
        let root_identity = authority_root_identity_from_file(&root_directory)?;
        verify_authority_root_binding(&canonical_root, root_identity, &root_directory)?;
        let lock_path = canonical_root.join(LOCK_FILE_NAME);
        refuse_symlink(&lock_path)?;
        let registry = OWNER_LEASE_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
        let mut registry_guard = registry.lock();
        if registry_guard
            .get(&lock_path)
            .and_then(Weak::upgrade)
            .is_some()
        {
            return Err(AuthorityRuntimeError::OwnerLeaseBusy { path: lock_path });
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&lock_path)
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "open_authority_owner_lock",
                source,
            })?;
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            return Err(AuthorityRuntimeError::OwnerLeaseBusy { path: lock_path });
        }
        let token = Arc::new(OwnerLeaseToken {
            canonical_lock_path: lock_path.clone(),
            canonical_root_path: canonical_root,
            root_directory,
            root_identity,
            file,
        });
        registry_guard.insert(lock_path, Arc::downgrade(&token));
        drop(registry_guard);
        Ok(Self { _token: token })
    }

    fn canonical_root(&self) -> &Path {
        &self._token.canonical_root_path
    }

    fn verify_root_binding(&self) -> Result<(), AuthorityRuntimeError> {
        verify_authority_root_binding(
            &self._token.canonical_root_path,
            self._token.root_identity,
            &self._token.root_directory,
        )
    }
}

/// Non-Unix posture is deliberately fail-closed until an OS-native lifetime
/// exclusive lock has the same tested semantics as the Unix `flock` owner
/// lease. No runtime files are created on this path.
#[cfg(not(unix))]
#[derive(Clone, Debug)]
struct OwnerLease;

#[cfg(not(unix))]
impl OwnerLease {
    fn acquire(_root: &Path) -> Result<Self, AuthorityRuntimeError> {
        Err(AuthorityRuntimeError::OwnerLeaseUnavailable {
            detail: "authority runtime is fail-closed unavailable: no supported lifetime owner lease on this platform"
                .to_string(),
        })
    }

    fn canonical_root(&self) -> &Path {
        unreachable!("non-Unix authority owner lease is unavailable")
    }

    fn verify_root_binding(&self) -> Result<(), AuthorityRuntimeError> {
        Err(AuthorityRuntimeError::OwnerLeaseUnavailable {
            detail: "authority runtime root binding is unavailable on this platform".to_string(),
        })
    }
}

#[cfg(unix)]
fn authority_root_identity_from_file(
    directory: &File,
) -> Result<AuthorityRootIdentity, AuthorityRuntimeError> {
    let metadata = directory
        .metadata()
        .map_err(|source| AuthorityRuntimeError::Io {
            operation: "metadata_authority_runtime_root_descriptor",
            source,
        })?;
    if !metadata.is_dir() {
        return Err(AuthorityRuntimeError::RollbackDetected {
            detail: "authority runtime root descriptor is no longer a directory".to_string(),
        });
    }
    Ok(AuthorityRootIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(unix)]
fn verify_authority_root_binding(
    path: &Path,
    expected: AuthorityRootIdentity,
    directory: &File,
) -> Result<(), AuthorityRuntimeError> {
    let descriptor = authority_root_identity_from_file(directory)?;
    if descriptor != expected {
        return Err(AuthorityRuntimeError::RollbackDetected {
            detail: format!(
                "authority runtime root descriptor identity changed from {expected:?} to {descriptor:?}"
            ),
        });
    }
    let metadata = fs::symlink_metadata(path).map_err(|source| AuthorityRuntimeError::Io {
        operation: "inspect_authority_runtime_root_binding",
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AuthorityRuntimeError::RollbackDetected {
            detail: format!(
                "authority runtime root binding is no longer a regular directory: {}",
                path.display()
            ),
        });
    }
    let observed = AuthorityRootIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    };
    if observed != expected {
        return Err(AuthorityRuntimeError::RollbackDetected {
            detail: format!(
                "authority runtime root identity for {} changed from {expected:?} to {observed:?}",
                path.display()
            ),
        });
    }
    Ok(())
}

#[derive(Debug)]
pub enum AuthorityRuntimeError {
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
    Json(serde_json::Error),
    Canonical(CanonicalError),
    Crypto(AuthorityCryptoError),
    Replay(ReplayLedgerError),
    Catalog(ActionCatalogError),
    Policy(PolicyError),
    MissingAuthorityRecord {
        path: PathBuf,
    },
    AlreadyBootstrapped {
        path: PathBuf,
    },
    CorruptState {
        detail: String,
    },
    CorruptJournal {
        detail: String,
    },
    CorruptReplay {
        detail: String,
    },
    CorruptTransitionDescriptor {
        detail: String,
    },
    RollbackDetected {
        detail: String,
    },
    ActivationConflict {
        detail: String,
    },
    OwnerLeaseBusy {
        path: PathBuf,
    },
    OwnerLeaseUnavailable {
        detail: String,
    },
    InvalidContract {
        detail: String,
    },
    ProtectedEpoch {
        detail: String,
    },
    ConcurrentModification {
        detail: String,
    },
    Poisoned,
    ChallengeNotFound {
        challenge_id: String,
    },
    ChallengeExpired {
        challenge_id: String,
    },
    ChallengeConsumed {
        challenge_id: String,
    },
    DuplicateChallengeNonce,
    SessionRegistryCapacity {
        registry: &'static str,
    },
    SessionNotFound {
        session_id: String,
    },
    SessionExpired {
        session_id: String,
    },
    SessionContextMismatch,
    SessionKeyInactive {
        detail: String,
    },
    UnknownAction {
        action: String,
    },
    UnreachableIngress {
        action: String,
        ingress: Ingress,
    },
    UncoveredAuthorityFloor {
        action: String,
        floor: AuthorityFloor,
    },
    IssuanceFrozen,
    AutonomyAdmissionOwnerAlreadyInstalled,
    AutonomyAdmissionUnavailable,
    AutonomyAdmission {
        detail: String,
    },
    AutonomyMirrorMismatch {
        field: &'static str,
    },
    AuthorityModeMismatch {
        mode: ActiveMode,
        variant: AuthorityVariant,
    },
    PositiveAuthorityRequired,
    SafetyAuthorityRequired,
    SafetyVerifierUnavailable,
    ServiceIdentityVerifierUnavailable,
    ServiceIdentityVerification {
        detail: String,
    },
    SafetyVerification {
        detail: String,
    },
    BindingMismatch {
        field: &'static str,
    },
    FaultInjected {
        point: &'static str,
    },
}

impl AuthorityRuntimeError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ChallengeNotFound { .. } => "authority_session_challenge_not_found",
            Self::ChallengeExpired { .. } => "authority_session_challenge_expired",
            Self::ChallengeConsumed { .. } => "authority_session_challenge_consumed",
            Self::DuplicateChallengeNonce => "authority_session_challenge_replay",
            Self::SessionRegistryCapacity { .. } => "authority_session_capacity_exceeded",
            Self::SessionNotFound { .. } => "authority_session_not_found",
            Self::SessionExpired { .. } => "authority_session_expired",
            Self::SessionContextMismatch => "authority_session_context_mismatch",
            Self::SessionKeyInactive { .. } => "authority_session_key_inactive",
            Self::Replay(_) => "authority_replay_refused",
            Self::Crypto(_) => "authority_crypto_refused",
            Self::IssuanceFrozen => "authority_issuance_frozen",
            Self::AutonomyAdmissionOwnerAlreadyInstalled => {
                "authority_autonomy_owner_already_installed"
            }
            Self::AutonomyAdmissionUnavailable => "authority_autonomy_admission_not_installed",
            Self::AutonomyAdmission { .. } => "authority_autonomy_admission_refused",
            Self::AutonomyMirrorMismatch { .. } => "authority_autonomy_mirror_mismatch_frozen",
            Self::BindingMismatch { .. } => "authority_binding_mismatch",
            Self::AuthorityModeMismatch { .. }
            | Self::PositiveAuthorityRequired
            | Self::SafetyAuthorityRequired
            | Self::UncoveredAuthorityFloor { .. }
            | Self::UnreachableIngress { .. }
            | Self::UnknownAction { .. } => "authority_policy_refused",
            Self::ServiceIdentityVerifierUnavailable | Self::SafetyVerifierUnavailable => {
                "authority_verifier_unavailable"
            }
            Self::OwnerLeaseBusy { .. }
            | Self::OwnerLeaseUnavailable { .. }
            | Self::ProtectedEpoch { .. } => "authority_runtime_unavailable",
            Self::Poisoned
            | Self::CorruptState { .. }
            | Self::CorruptJournal { .. }
            | Self::CorruptReplay { .. }
            | Self::CorruptTransitionDescriptor { .. }
            | Self::RollbackDetected { .. }
            | Self::ConcurrentModification { .. } => "authority_runtime_corruption",
            _ => "authority_runtime_refused",
        }
    }
}

impl fmt::Display for AuthorityRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, source } => {
                write!(
                    formatter,
                    "authority runtime I/O failed during {operation}: {source}"
                )
            }
            Self::Json(error) => write!(formatter, "authority runtime JSON failed: {error}"),
            Self::Canonical(error) => {
                write!(
                    formatter,
                    "authority runtime canonicalization failed: {error}"
                )
            }
            Self::Crypto(error) => write!(formatter, "authority crypto rejected input: {error}"),
            Self::Replay(error) => write!(formatter, "authority replay rejected input: {error}"),
            Self::Catalog(error) => write!(formatter, "action catalog is invalid: {error}"),
            Self::Policy(error) => write!(formatter, "action policy registry is invalid: {error}"),
            Self::MissingAuthorityRecord { path } => {
                write!(
                    formatter,
                    "authoritative state record is missing: {}",
                    path.display()
                )
            }
            Self::AlreadyBootstrapped { path } => {
                write!(
                    formatter,
                    "authority runtime already exists: {}",
                    path.display()
                )
            }
            Self::CorruptState { detail } => write!(formatter, "corrupt authority state: {detail}"),
            Self::CorruptJournal { detail } => {
                write!(formatter, "corrupt authority journal: {detail}")
            }
            Self::CorruptReplay { detail } => {
                write!(formatter, "corrupt authority replay ledger: {detail}")
            }
            Self::CorruptTransitionDescriptor { detail } => {
                write!(
                    formatter,
                    "corrupt authority transition descriptor: {detail}"
                )
            }
            Self::RollbackDetected { detail } => {
                write!(formatter, "authority rollback detected: {detail}")
            }
            Self::ActivationConflict { detail } => {
                write!(formatter, "authority activation conflict: {detail}")
            }
            Self::OwnerLeaseBusy { path } => {
                write!(
                    formatter,
                    "authority owner lease is busy: {}",
                    path.display()
                )
            }
            Self::OwnerLeaseUnavailable { detail } => {
                write!(formatter, "authority owner lease unavailable: {detail}")
            }
            Self::InvalidContract { detail } => {
                write!(formatter, "invalid authority contract: {detail}")
            }
            Self::ProtectedEpoch { detail } => {
                write!(formatter, "protected epoch backend failed: {detail}")
            }
            Self::ConcurrentModification { detail } => {
                write!(formatter, "authority concurrent modification: {detail}")
            }
            Self::Poisoned => formatter
                .write_str("authority runtime is poisoned after an ambiguous durable transition"),
            Self::ChallengeNotFound { challenge_id } => {
                write!(formatter, "session challenge not found: {challenge_id}")
            }
            Self::ChallengeExpired { challenge_id } => {
                write!(formatter, "session challenge expired: {challenge_id}")
            }
            Self::ChallengeConsumed { challenge_id } => {
                write!(
                    formatter,
                    "session challenge already consumed: {challenge_id}"
                )
            }
            Self::DuplicateChallengeNonce => {
                formatter.write_str("session challenge nonce was already issued")
            }
            Self::SessionRegistryCapacity { registry } => {
                write!(
                    formatter,
                    "authority session registry capacity exceeded: {registry}"
                )
            }
            Self::SessionNotFound { session_id } => {
                write!(formatter, "authenticated session not found: {session_id}")
            }
            Self::SessionExpired { session_id } => {
                write!(formatter, "authenticated session expired: {session_id}")
            }
            Self::SessionContextMismatch => {
                formatter.write_str("authenticated session context digest mismatch")
            }
            Self::SessionKeyInactive { detail } => {
                write!(formatter, "authenticated session key is inactive: {detail}")
            }
            Self::UnknownAction { action } => {
                write!(formatter, "unknown semantic action: {action}")
            }
            Self::UnreachableIngress { action, ingress } => {
                write!(formatter, "action {action} is unreachable from {ingress:?}")
            }
            Self::UncoveredAuthorityFloor { action, floor } => {
                write!(
                    formatter,
                    "action {action} has uncovered authority floor {floor:?}"
                )
            }
            Self::IssuanceFrozen => formatter.write_str("positive authority issuance is frozen"),
            Self::AutonomyAdmissionOwnerAlreadyInstalled => {
                formatter.write_str("constitutional autonomy admission owner is already installed")
            }
            Self::AutonomyAdmissionUnavailable => {
                formatter.write_str("constitutional autonomy admission owner is not installed")
            }
            Self::AutonomyAdmission { detail } => {
                write!(
                    formatter,
                    "constitutional autonomy admission refused: {detail}"
                )
            }
            Self::AutonomyMirrorMismatch { field } => write!(
                formatter,
                "G2/G9 authority mirror mismatch at {field}; global positive issuance was frozen"
            ),
            Self::AuthorityModeMismatch { mode, variant } => write!(
                formatter,
                "authority variant {variant:?} is invalid in mode {mode:?}"
            ),
            Self::PositiveAuthorityRequired => {
                formatter.write_str("positive authority input is required")
            }
            Self::SafetyAuthorityRequired => {
                formatter.write_str("disjoint actuator-only safety authority is required")
            }
            Self::SafetyVerifierUnavailable => {
                formatter.write_str("no safety actuator verifier is installed")
            }
            Self::ServiceIdentityVerifierUnavailable => {
                formatter.write_str("no pinned service identity verifier is installed")
            }
            Self::ServiceIdentityVerification { detail } => {
                write!(formatter, "service identity verification failed: {detail}")
            }
            Self::SafetyVerification { detail } => {
                write!(formatter, "safety actuator verification failed: {detail}")
            }
            Self::BindingMismatch { field } => {
                write!(formatter, "authority binding mismatch for {field}")
            }
            Self::FaultInjected { point } => {
                write!(
                    formatter,
                    "authority transition fault injected after {point}"
                )
            }
        }
    }
}

impl Error for AuthorityRuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json(error) => Some(error),
            Self::Canonical(error) => Some(error),
            Self::Crypto(error) => Some(error),
            Self::Replay(error) => Some(error),
            Self::Catalog(error) => Some(error),
            Self::Policy(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for AuthorityRuntimeError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<CanonicalError> for AuthorityRuntimeError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<AuthorityCryptoError> for AuthorityRuntimeError {
    fn from(error: AuthorityCryptoError) -> Self {
        Self::Crypto(error)
    }
}

impl From<ReplayLedgerError> for AuthorityRuntimeError {
    fn from(error: ReplayLedgerError) -> Self {
        Self::Replay(error)
    }
}

impl From<ActionCatalogError> for AuthorityRuntimeError {
    fn from(error: ActionCatalogError) -> Self {
        Self::Catalog(error)
    }
}

impl From<PolicyError> for AuthorityRuntimeError {
    fn from(error: PolicyError) -> Self {
        Self::Policy(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorityVerificationAssurance {
    ControlVerifiedEd25519,
    SoftwareTestOnlyNotProven,
}

trait PositiveAuthorityVerifier: Send {
    fn assurance(&self) -> AuthorityVerificationAssurance;
    fn precheck(
        &self,
        capability: &AuthorityCapabilityV1,
        keys: &VerificationKeyRegistryV1,
        context: CapabilityVerificationContext<'_>,
    ) -> Result<(), AuthorityRuntimeError>;
    fn verify_once(
        &mut self,
        capability: &AuthorityCapabilityV1,
        keys: &VerificationKeyRegistryV1,
        context: CapabilityVerificationContext<'_>,
        replay: &mut dyn ReplayLedger,
    ) -> Result<PositiveAuthorityProofV1, AuthorityRuntimeError>;
}

#[derive(Clone, Debug)]
struct PositiveAuthorityProofV1 {
    signed_body_digest: String,
    key_id: String,
    subject_id: String,
    replay: ReplayReceiptV1,
}

struct PositiveVerificationRequestV1<'a> {
    capability: &'a AuthorityCapabilityV1,
    keys: &'a VerificationKeyRegistryV1,
    expected_subject_id: &'a str,
    expected_payload_digest: &'a str,
    expected_action: &'a str,
    expected_mission_id: Option<&'a str>,
    expected_mission_head_id: Option<&'a str>,
    now_ms: u64,
}

struct ControlCryptoAuthorityVerifier;

impl PositiveAuthorityVerifier for ControlCryptoAuthorityVerifier {
    fn assurance(&self) -> AuthorityVerificationAssurance {
        AuthorityVerificationAssurance::ControlVerifiedEd25519
    }

    fn precheck(
        &self,
        capability: &AuthorityCapabilityV1,
        keys: &VerificationKeyRegistryV1,
        context: CapabilityVerificationContext<'_>,
    ) -> Result<(), AuthorityRuntimeError> {
        verify_capability(capability, keys, context)?;
        Ok(())
    }

    fn verify_once(
        &mut self,
        capability: &AuthorityCapabilityV1,
        keys: &VerificationKeyRegistryV1,
        context: CapabilityVerificationContext<'_>,
        replay: &mut dyn ReplayLedger,
    ) -> Result<PositiveAuthorityProofV1, AuthorityRuntimeError> {
        let verified = verify_capability_once(capability, keys, context, replay)?;
        Ok(PositiveAuthorityProofV1 {
            signed_body_digest: verified.authority.signed_body_digest,
            key_id: verified.authority.key_id,
            subject_id: verified.authority.subject_id,
            replay: verified.replay,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionChallengeCoreV1 {
    pub challenge_id: String,
    pub subject_id: String,
    pub key_id: String,
    pub app_host_identity: String,
    pub audience: String,
    pub organism_id: String,
    pub brain_id: String,
    pub session_context_digest: String,
    pub nonce: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionChallengeV1 {
    pub schema: String,
    pub core: SessionChallengeCoreV1,
    pub challenge_digest: String,
}

impl SessionChallengeV1 {
    fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(SESSION_CHALLENGE_DIGEST_DOMAIN, &self.core)
    }

    fn seal(&mut self) -> Result<(), CanonicalError> {
        self.challenge_digest = self.compute_digest()?;
        Ok(())
    }

    fn validate(&self, now_ms: u64) -> Result<(), AuthorityRuntimeError> {
        if self.schema != SESSION_CHALLENGE_SCHEMA {
            return Err(AuthorityRuntimeError::InvalidContract {
                detail: "unsupported session challenge schema".to_string(),
            });
        }
        for (field, value) in [
            ("challenge_id", self.core.challenge_id.as_str()),
            ("challenge.subject_id", self.core.subject_id.as_str()),
            ("challenge.key_id", self.core.key_id.as_str()),
            (
                "challenge.app_host_identity",
                self.core.app_host_identity.as_str(),
            ),
            ("challenge.audience", self.core.audience.as_str()),
            ("challenge.organism_id", self.core.organism_id.as_str()),
            ("challenge.brain_id", self.core.brain_id.as_str()),
            ("challenge.nonce", self.core.nonce.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        require_digest(
            "challenge.session_context_digest",
            &self.core.session_context_digest,
        )?;
        require_digest("challenge.challenge_digest", &self.challenge_digest)?;
        if self.core.issued_at > now_ms || self.core.expires_at <= self.core.issued_at {
            return Err(AuthorityRuntimeError::InvalidContract {
                detail: "session challenge has invalid time window".to_string(),
            });
        }
        if now_ms >= self.core.expires_at {
            return Err(AuthorityRuntimeError::ChallengeExpired {
                challenge_id: self.core.challenge_id.clone(),
            });
        }
        if self.compute_digest()? != self.challenge_digest {
            return Err(AuthorityRuntimeError::InvalidContract {
                detail: "session challenge digest mismatch".to_string(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedSessionV1 {
    pub session_id: String,
    pub subject_id: String,
    pub key_id: String,
    pub app_host_identity: String,
    pub audience: String,
    pub session_context_digest: String,
    pub key_registry_epoch: u64,
    pub authenticated_at: u64,
    pub expires_at: u64,
    pub authentication_body_digest: String,
    pub verification_assurance: AuthorityVerificationAssurance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedServiceIdentityV1 {
    pub service_id: String,
    pub subject_id: String,
    pub key_id: String,
    pub role: Role,
    pub organism_id: String,
    pub brain_id: String,
    pub audience: String,
    pub identity_key_binary_policy_digest: String,
    pub allowed_actions: BTreeSet<ActionId>,
    pub expires_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceIdentityAssertionCoreV1 {
    pub service_id: String,
    pub subject_id: String,
    pub key_id: String,
    pub role: Role,
    pub organism_id: String,
    pub brain_id: String,
    pub audience: String,
    pub identity_key_binary_policy_digest: String,
    pub action: ActionId,
    pub object_digest: String,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
    pub transport_session_id: String,
    pub ingress_context_digest: String,
    pub nonce: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceIdentityAssertionV1 {
    pub schema: String,
    pub core: ServiceIdentityAssertionCoreV1,
    pub signature: OpaqueSignature,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServiceIdentityVerificationAssurance {
    SoftwareTestOnlyNotProven,
    ProductionCryptographicPinnedIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedServiceIdentityV1 {
    pub signed_body_digest: String,
    pub assurance: ServiceIdentityVerificationAssurance,
}

/// Verifies the service signature and its platform/binary identity. The
/// runtime independently rechecks the returned digest against the exact
/// assertion and the owner-pinned registry entry.
pub trait ServiceIdentityVerifier: Send {
    fn verify(
        &mut self,
        assertion: &ServiceIdentityAssertionV1,
        pinned: &PinnedServiceIdentityV1,
    ) -> Result<VerifiedServiceIdentityV1, String>;
}

impl ServiceIdentityAssertionV1 {
    pub fn signed_body_digest(&self) -> Result<String, AuthorityRuntimeError> {
        Ok(digest_canonical(
            SERVICE_IDENTITY_ASSERTION_DIGEST_DOMAIN,
            &self.core,
        )?)
    }
}

#[derive(Clone, Debug)]
struct PendingSessionChallenge {
    challenge: SessionChallengeV1,
    consumed: bool,
}

const MAX_PENDING_SESSION_CHALLENGES: usize = 4_096;
const MAX_ISSUED_SESSION_NONCES: usize = 8_192;
const MAX_AUTHENTICATED_SESSIONS: usize = 4_096;

#[derive(Default)]
struct AuthenticatedSessionRegistry {
    challenges: BTreeMap<String, PendingSessionChallenge>,
    /// A nonce remains reserved until the challenge window expires even after
    /// successful authentication. This prevents immediate replay while expiry
    /// GC and hard admission caps keep the process-local registry bounded.
    issued_nonces: BTreeMap<String, u64>,
    sessions: BTreeMap<String, AuthenticatedSessionV1>,
}

impl AuthenticatedSessionRegistry {
    fn gc(&mut self, now_ms: u64) {
        self.challenges
            .retain(|_, pending| now_ms < pending.challenge.core.expires_at);
        self.issued_nonces
            .retain(|_, expires_at| now_ms < *expires_at);
        self.sessions
            .retain(|_, session| now_ms < session.expires_at);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SafetyVerifierAssurance {
    SoftwareTestOnlyNotProven,
    ProductionCryptographic,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyActuatorAttemptCoreV1 {
    pub attempt_id: String,
    pub actuator_subject_id: String,
    pub actuator_key_id: String,
    pub actuator_identity_key_binary_policy_digest: String,
    pub action: ActionId,
    pub payload_digest: String,
    pub negative_effects: BTreeSet<Effect>,
    pub constitution_epoch: u64,
    pub autonomy_epoch: u64,
    pub nonce: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyActuatorAttemptV1 {
    pub schema: String,
    pub core: SafetyActuatorAttemptCoreV1,
    pub signature: OpaqueSignature,
}

impl SafetyActuatorAttemptV1 {
    pub fn signed_body_digest(&self) -> Result<String, AuthorityRuntimeError> {
        Ok(digest_canonical(
            SAFETY_ACTUATOR_ATTEMPT_DIGEST_DOMAIN,
            &self.core,
        )?)
    }

    fn validate_structural(&self, now_ms: u64) -> Result<(), AuthorityRuntimeError> {
        if self.schema != SAFETY_ACTUATOR_ATTEMPT_SCHEMA {
            return Err(AuthorityRuntimeError::InvalidContract {
                detail: "unsupported safety actuator attempt schema".to_string(),
            });
        }
        for (field, value) in [
            ("safety.attempt_id", self.core.attempt_id.as_str()),
            (
                "safety.actuator_subject_id",
                self.core.actuator_subject_id.as_str(),
            ),
            ("safety.actuator_key_id", self.core.actuator_key_id.as_str()),
            ("safety.action", self.core.action.as_str()),
            ("safety.nonce", self.core.nonce.as_str()),
            ("safety.signature", self.signature.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        require_digest(
            "safety.actuator_identity_key_binary_policy_digest",
            &self.core.actuator_identity_key_binary_policy_digest,
        )?;
        require_digest("safety.payload_digest", &self.core.payload_digest)?;
        if self.core.negative_effects.is_empty()
            || self
                .core
                .negative_effects
                .iter()
                .any(|effect| !effect.is_negative_safety())
        {
            return Err(AuthorityRuntimeError::InvalidContract {
                detail: "safety attempt contains a positive or empty effect set".to_string(),
            });
        }
        if self.core.issued_at > now_ms
            || self.core.expires_at <= self.core.issued_at
            || now_ms >= self.core.expires_at
        {
            return Err(AuthorityRuntimeError::InvalidContract {
                detail: "safety attempt has invalid or expired time window".to_string(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedSafetyActuatorV1 {
    pub signed_body_digest: String,
    pub key_id: String,
    pub subject_id: String,
    pub assurance: SafetyVerifierAssurance,
}

/// Security-critical injection boundary. A production implementation must
/// verify the pinned actuator key/signature and return
/// `ProductionCryptographic`; the runtime still independently rechecks all
/// semantic bindings and consumes the nonce in its own replay ledger.
pub trait SafetyActuatorVerifier: Send {
    fn verify(
        &mut self,
        attempt: &SafetyActuatorAttemptV1,
    ) -> Result<VerifiedSafetyActuatorV1, String>;
}

#[derive(Clone, Debug)]
pub struct SessionChallengeRequestV1 {
    pub challenge_id: String,
    pub subject_id: String,
    pub key_id: String,
    pub app_host_identity: String,
    pub session_context_digest: String,
    pub nonce: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityRuntimeStatusV1 {
    pub state: AuthorityRuntimeStateV1,
    pub protected_epoch_assurance: ProtectedEpochAssurance,
    pub positive_verification_assurance: AuthorityVerificationAssurance,
    pub semantic_catalog_entries: usize,
    /// This foundation gates the semantic catalog only. Transport schema parity
    /// must be proven by the server integration layer before claiming complete
    /// coverage of every `all_tool_schemas()` surface.
    pub transport_schema_parity_proven: bool,
    /// Atomic replacement is proven for the state record, not for the replay,
    /// journal, protected epoch, and state as one indivisible transaction.
    pub multi_artifact_atomicity_proven: bool,
    /// Exact descriptor-bound boundary failures are recovered old-or-new.
    /// Corrupt descriptors, partial/unbound tails, and unknown epoch values
    /// remain fail closed instead of being guessed into a successful outcome.
    pub automatic_crash_recovery_proven: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailClosedAuthorityProjectionV1 {
    pub active_mode: ActiveMode,
    pub issuance_frozen: bool,
    pub may_authorize_positive: bool,
    pub full_autonomy: bool,
    pub reason: String,
}

impl FailClosedAuthorityProjectionV1 {
    pub fn from_open_error(error: &AuthorityRuntimeError) -> Self {
        Self {
            active_mode: ActiveMode::HumanGated,
            issuance_frozen: true,
            may_authorize_positive: false,
            full_autonomy: false,
            reason: error.to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AuthorityAuthorizationRequestV1 {
    /// Cryptographically authenticated G2 session id. This is deliberately not
    /// the REST bearer identity or Streamable-HTTP `Mcp-Session-Id`.
    pub session_id: Option<String>,
    pub session_context_digest: Option<String>,
    /// Trusted wire facts injected by the owner transport and persisted into
    /// the authorization receipt for one-shot lease consumption.
    pub transport_session_id: String,
    pub ingress_context_digest: String,
    pub ingress: Ingress,
    pub action: ActionId,
    pub payload_digest: String,
    pub requested_effects: BTreeSet<Effect>,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
    pub now_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositiveSovereignAuthorityMetadataV1 {
    pub role: Role,
    pub capability_kind: CapabilityKind,
    pub authority_decision_digest: String,
    pub applicable_grant_id: Option<String>,
    pub applicable_tier: Option<AutonomyTier>,
}

pub enum AuthorityInputV1<'a> {
    OrdinarySession {
        keys: &'a VerificationKeyRegistryV1,
        role: Role,
    },
    Positive {
        capability: &'a AuthorityCapabilityV1,
        keys: &'a VerificationKeyRegistryV1,
    },
    PositiveSovereign {
        capability: &'a AuthorityCapabilityV1,
        keys: &'a VerificationKeyRegistryV1,
        metadata: &'a PositiveSovereignAuthorityMetadataV1,
        autonomy_evidence: Option<&'a AutonomyAuthorityEvidenceV1>,
    },
    ServiceIdentity {
        assertion: &'a ServiceIdentityAssertionV1,
    },
    Safety {
        attempt: &'a SafetyActuatorAttemptV1,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorizationAuthorityV1 {
    OrdinarySession {
        assurance: AuthorityVerificationAssurance,
    },
    Positive {
        variant: AuthorityVariant,
        assurance: AuthorityVerificationAssurance,
    },
    Autonomous {
        variant: AuthorityVariant,
        capability_assurance: AuthorityVerificationAssurance,
        admission_receipt_digest: String,
    },
    ServiceIdentity {
        service_id: String,
        assurance: ServiceIdentityVerificationAssurance,
    },
    SafetyActuator {
        assurance: SafetyVerifierAssurance,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityAuthorizationReceiptCoreV1 {
    pub organism_id: String,
    pub repo_id: String,
    pub brain_id: String,
    pub subject_id: String,
    pub role: Role,
    pub capability_id: String,
    pub capability_kind: Option<CapabilityKind>,
    pub verified_object_digest: String,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
    pub transport_session_id: String,
    pub ingress_context_digest: String,
    pub action: ActionId,
    pub ingress: Ingress,
    pub complete_effects: BTreeSet<Effect>,
    pub active_mode: ActiveMode,
    pub constitution_digest: String,
    pub constitution_epoch: u64,
    pub autonomy_epoch: u64,
    pub protected_epoch_at_decision: u64,
    pub policy_registry_digest: String,
    pub exact_policy_tuple: ReachablePolicyTupleV1,
    pub authority_decision_digest: Option<String>,
    pub autonomy_admission_receipt_digest: Option<String>,
    pub autonomy_committed_state_digest: Option<String>,
    pub autonomy_protected_root_digest: Option<String>,
    pub authority: AuthorizationAuthorityV1,
    pub authority_body_digest: String,
    pub replay_sequence: u64,
    pub journal_sequence: u64,
    pub journal_root_digest: String,
    pub protected_epoch: u64,
    pub authorized_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityAuthorizationReceiptV1 {
    pub schema: String,
    pub core: AuthorityAuthorizationReceiptCoreV1,
    pub receipt_digest: String,
    pub issuer: String,
    pub key_id: String,
    pub algorithm: String,
    pub signature: OpaqueSignature,
}

impl AuthorityAuthorizationReceiptV1 {
    fn new(core: AuthorityAuthorizationReceiptCoreV1) -> Result<Self, AuthorityRuntimeError> {
        let receipt_digest = digest_canonical(AUTHORIZATION_RECEIPT_DIGEST_DOMAIN, &core)?;
        Ok(Self {
            schema: AUTHORIZATION_RECEIPT_SCHEMA.to_string(),
            core,
            receipt_digest,
            issuer: "NOT_INSTALLED".to_string(),
            key_id: "NOT_INSTALLED".to_string(),
            algorithm: "NOT_INSTALLED".to_string(),
            signature: OpaqueSignature::new("NOT_INSTALLED"),
        })
    }

    /// Complete sealed receipt including its core digest and signer metadata,
    /// excluding only the signature. `receipt_digest` itself covers only core,
    /// so signing this payload is explicit and non-circular.
    pub fn canonical_signature_payload(&self) -> Result<Vec<u8>, AuthorityRuntimeError> {
        let mut value = serde_json::to_value(self)?;
        value
            .as_object_mut()
            .expect("authorization receipt serializes as an object")
            .remove("signature");
        Ok(canonical_json(&value)?)
    }

    #[cfg(test)]
    pub(crate) fn new_for_broker_test(core: AuthorityAuthorizationReceiptCoreV1) -> Self {
        Self::new(core).expect("test authorization receipt must seal")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum AuthorityTransitionPhaseV1 {
    Prepared,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorityTransitionDescriptorCoreV1 {
    transaction_id: String,
    phase: AuthorityTransitionPhaseV1,
    prior_state: Option<AuthorityRuntimeStateV1>,
    next_state: AuthorityRuntimeStateV1,
    journal_record: AuthorityJournalRecordV1,
    replay_record: Option<AuthorityReplayRecordV1>,
    prior_journal_length: u64,
    prior_replay_length: u64,
    prior_replay_tail_digest: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorityTransitionDescriptorV1 {
    schema: String,
    core: AuthorityTransitionDescriptorCoreV1,
    descriptor_digest: String,
}

impl AuthorityTransitionDescriptorV1 {
    fn new(
        prior_state: Option<AuthorityRuntimeStateV1>,
        next_state: AuthorityRuntimeStateV1,
        journal_record: AuthorityJournalRecordV1,
        replay_record: Option<AuthorityReplayRecordV1>,
        prior_journal_length: u64,
        prior_replay_length: u64,
        prior_replay_tail_digest: Option<String>,
    ) -> Result<Self, AuthorityRuntimeError> {
        let transaction_id = transition_id(
            prior_state.as_ref(),
            &next_state,
            &journal_record,
            replay_record.as_ref(),
        )?;
        let mut descriptor = Self {
            schema: AUTHORITY_TRANSITION_DESCRIPTOR_SCHEMA.to_string(),
            core: AuthorityTransitionDescriptorCoreV1 {
                transaction_id,
                phase: AuthorityTransitionPhaseV1::Prepared,
                prior_state,
                next_state,
                journal_record,
                replay_record,
                prior_journal_length,
                prior_replay_length,
                prior_replay_tail_digest,
            },
            descriptor_digest: String::new(),
        };
        descriptor.descriptor_digest = descriptor.compute_digest()?;
        Ok(descriptor)
    }

    fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(AUTHORITY_TRANSITION_DESCRIPTOR_DIGEST_DOMAIN, &self.core)
    }

    fn validate(
        &self,
        config: &AuthorityRuntimeConfig,
        catalog: &ActionCatalogV1,
    ) -> Result<(), AuthorityRuntimeError> {
        let corrupt = |detail: &str| AuthorityRuntimeError::CorruptTransitionDescriptor {
            detail: detail.to_string(),
        };
        if self.schema != AUTHORITY_TRANSITION_DESCRIPTOR_SCHEMA
            || self.core.phase != AuthorityTransitionPhaseV1::Prepared
            || self.compute_digest()? != self.descriptor_digest
        {
            return Err(corrupt("descriptor schema, phase, or self-digest mismatch"));
        }
        if transition_id(
            self.core.prior_state.as_ref(),
            &self.core.next_state,
            &self.core.journal_record,
            self.core.replay_record.as_ref(),
        )? != self.core.transaction_id
        {
            return Err(corrupt(
                "transaction id does not bind exact old/new artifacts",
            ));
        }
        self.core.next_state.validate(config, catalog)?;
        if let Some(prior) = &self.core.prior_state {
            prior.validate(config, catalog)?;
            if self.core.next_state.core.revision != prior.core.revision.saturating_add(1)
                || self.core.next_state.core.protected_epoch
                    != prior.core.protected_epoch.saturating_add(1)
                || self.core.next_state.core.journal_sequence
                    != prior.core.journal_sequence.saturating_add(1)
            {
                return Err(corrupt(
                    "next state does not advance prior state exactly once",
                ));
            }
            if self.core.next_state.core.updated_at < prior.core.updated_at {
                return Err(corrupt("next state timestamp regresses"));
            }
        } else if self.core.next_state.core.revision != 0
            || self.core.next_state.core.protected_epoch != 1
            || self.core.next_state.core.journal_sequence != 1
        {
            return Err(corrupt("bootstrap descriptor does not start at epoch one"));
        }
        let prior_journal_sequence = self
            .core
            .prior_state
            .as_ref()
            .map_or(0, |state| state.core.journal_sequence);
        let prior_journal_tail = self
            .core
            .prior_state
            .as_ref()
            .map(|state| state.core.journal_root_digest.as_str());
        self.core
            .journal_record
            .validate(prior_journal_sequence.saturating_add(1), prior_journal_tail)?;
        if self.core.journal_record.record_digest != self.core.next_state.core.journal_root_digest
            || self.core.journal_record.core.sequence != self.core.next_state.core.journal_sequence
            || self.core.journal_record.core.protected_epoch
                != self.core.next_state.core.protected_epoch
        {
            return Err(corrupt("journal record does not bind next state"));
        }
        if self.core.journal_record.core.recorded_at != self.core.next_state.core.updated_at {
            return Err(corrupt(
                "journal and next state timestamps are not the same transition time",
            ));
        }
        let prior_replay_sequence = self
            .core
            .prior_state
            .as_ref()
            .map_or(0, |state| state.core.replay_sequence);
        let prior_state_replay_root = self
            .core
            .prior_state
            .as_ref()
            .and_then(|state| state.core.replay_root_digest.as_deref());
        if self.core.prior_replay_tail_digest.as_deref() != prior_state_replay_root {
            return Err(corrupt(
                "descriptor prior replay tail is not bound by prior state",
            ));
        }
        if prior_replay_sequence == 0 && self.core.prior_replay_tail_digest.is_some() {
            return Err(corrupt("zero replay sequence has a tail digest"));
        }
        if prior_replay_sequence > 0 {
            let tail = self
                .core
                .prior_replay_tail_digest
                .as_deref()
                .ok_or_else(|| corrupt("nonzero replay sequence has no tail digest"))?;
            require_digest("descriptor.prior_replay_tail_digest", tail)?;
        }
        match &self.core.replay_record {
            Some(record) => {
                record.validate(
                    prior_replay_sequence.saturating_add(1),
                    self.core.prior_replay_tail_digest.as_deref(),
                )?;
                if self.core.next_state.core.replay_sequence != record.sequence {
                    return Err(corrupt("replay record does not bind next state"));
                }
                if self.core.next_state.core.replay_root_digest.as_deref()
                    != Some(record.record_digest.as_str())
                {
                    return Err(corrupt("replay root does not bind next state"));
                }
            }
            None if self.core.next_state.core.replay_sequence != prior_replay_sequence => {
                return Err(corrupt("next replay sequence advanced without a record"));
            }
            None if self.core.next_state.core.replay_root_digest.as_deref()
                != prior_state_replay_root =>
            {
                return Err(corrupt("next replay root changed without a record"));
            }
            None => {}
        }
        Ok(())
    }

    fn prior_snapshot(&self) -> Option<ProtectedEpochSnapshotV1> {
        self.core
            .prior_state
            .as_ref()
            .map(AuthorityRuntimeStateV1::protected_snapshot)
    }
}

fn transition_id(
    prior_state: Option<&AuthorityRuntimeStateV1>,
    next_state: &AuthorityRuntimeStateV1,
    journal_record: &AuthorityJournalRecordV1,
    replay_record: Option<&AuthorityReplayRecordV1>,
) -> Result<String, CanonicalError> {
    digest_canonical(
        "m1nd-authority-transition-id-v1",
        &(
            prior_state.map(|state| state.record_digest.as_str()),
            next_state.record_digest.as_str(),
            journal_record.record_digest.as_str(),
            replay_record.map(|record| record.record_digest.as_str()),
        ),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransitionFaultPoint {
    Descriptor,
    Replay,
    Journal,
    ProtectedCas,
    State,
    Cleanup,
}

impl TransitionFaultPoint {
    fn label(self) -> &'static str {
        match self {
            Self::Descriptor => "descriptor",
            Self::Replay => "replay",
            Self::Journal => "journal",
            Self::ProtectedCas => "protected-cas",
            Self::State => "state",
            Self::Cleanup => "cleanup",
        }
    }
}

fn inject_transition_fault(
    configured: &mut Option<TransitionFaultPoint>,
    point: TransitionFaultPoint,
) -> Result<(), AuthorityRuntimeError> {
    if configured.as_ref() == Some(&point) {
        *configured = None;
        return Err(AuthorityRuntimeError::FaultInjected {
            point: point.label(),
        });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreparedRecoveryDisposition {
    None,
    RolledBackBeforeCommit,
    ForwardCompletedAfterCommit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExactTailDisposition {
    Prior,
    Prepared,
}

fn recover_prepared_transition(
    config: &AuthorityRuntimeConfig,
    catalog: &ActionCatalogV1,
    protected_backend: &mut dyn ProtectedEpochBackend,
) -> Result<PreparedRecoveryDisposition, AuthorityRuntimeError> {
    let descriptor_path = config.root.join(TRANSITION_DESCRIPTOR_FILE_NAME);
    refuse_symlink(&descriptor_path)?;
    if !descriptor_path.exists() {
        return Ok(PreparedRecoveryDisposition::None);
    }
    let descriptor = read_transition_descriptor(&descriptor_path)?;
    descriptor.validate(config, catalog)?;

    let journal_path = config.root.join(JOURNAL_FILE_NAME);
    let replay_path = config.root.join(REPLAY_FILE_NAME);
    for path in [&journal_path, &replay_path] {
        refuse_symlink(path)?;
        if !path.exists() {
            return Err(AuthorityRuntimeError::CorruptTransitionDescriptor {
                detail: format!(
                    "prepared descriptor references missing artifact {}",
                    path.display()
                ),
            });
        }
    }
    let journal_tail = classify_journal_tail(&journal_path, &descriptor)?;
    let replay_tail = classify_replay_tail(&replay_path, &descriptor)?;
    let disk_state = classify_descriptor_state(&config.root.join(STATE_FILE_NAME), &descriptor)?;
    let observed_protected = protected_backend
        .read_latest()
        .map_err(|detail| AuthorityRuntimeError::ProtectedEpoch { detail })?;
    let prior_snapshot = descriptor.prior_snapshot();
    let next_snapshot = descriptor.core.next_state.protected_snapshot();

    if observed_protected == prior_snapshot {
        if disk_state != ExactTailDisposition::Prior {
            return Err(AuthorityRuntimeError::CorruptTransitionDescriptor {
                detail: "next state exists before protected commit marker".to_string(),
            });
        }
        if journal_tail == ExactTailDisposition::Prepared {
            truncate_exact_tail(
                &journal_path,
                descriptor.core.prior_journal_length,
                "rollback_prepared_journal",
            )?;
        }
        if replay_tail == ExactTailDisposition::Prepared {
            truncate_exact_tail(
                &replay_path,
                descriptor.core.prior_replay_length,
                "rollback_prepared_replay",
            )?;
        }
        remove_transition_descriptor(&descriptor_path)?;
        return Ok(PreparedRecoveryDisposition::RolledBackBeforeCommit);
    }

    if observed_protected.as_ref() == Some(&next_snapshot) {
        if journal_tail != ExactTailDisposition::Prepared {
            return Err(AuthorityRuntimeError::CorruptTransitionDescriptor {
                detail: "protected commit advanced without exact prepared journal tail".to_string(),
            });
        }
        if descriptor.core.replay_record.is_some() && replay_tail != ExactTailDisposition::Prepared
        {
            return Err(AuthorityRuntimeError::CorruptTransitionDescriptor {
                detail: "protected commit advanced without exact prepared replay tail".to_string(),
            });
        }
        if disk_state == ExactTailDisposition::Prior {
            atomic_replace_json(
                &config.root.join(STATE_FILE_NAME),
                &descriptor.core.next_state,
            )?;
        }
        remove_transition_descriptor(&descriptor_path)?;
        return Ok(PreparedRecoveryDisposition::ForwardCompletedAfterCommit);
    }

    Err(AuthorityRuntimeError::RollbackDetected {
        detail: "protected epoch matches neither descriptor prior nor exact next snapshot"
            .to_string(),
    })
}

fn read_transition_descriptor(
    path: &Path,
) -> Result<AuthorityTransitionDescriptorV1, AuthorityRuntimeError> {
    refuse_symlink(path)?;
    let bytes = fs::read(path).map_err(|source| AuthorityRuntimeError::Io {
        operation: "read_transition_descriptor",
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        AuthorityRuntimeError::CorruptTransitionDescriptor {
            detail: format!("descriptor JSON is invalid: {error}"),
        }
    })
}

fn classify_descriptor_state(
    state_path: &Path,
    descriptor: &AuthorityTransitionDescriptorV1,
) -> Result<ExactTailDisposition, AuthorityRuntimeError> {
    refuse_symlink(state_path)?;
    if !state_path.exists() {
        return if descriptor.core.prior_state.is_none() {
            Ok(ExactTailDisposition::Prior)
        } else {
            Err(AuthorityRuntimeError::CorruptTransitionDescriptor {
                detail: "prior authoritative state disappeared during prepared transition"
                    .to_string(),
            })
        };
    }
    let state = read_authority_state(state_path)?;
    if descriptor.core.prior_state.as_ref() == Some(&state) {
        Ok(ExactTailDisposition::Prior)
    } else if descriptor.core.next_state == state {
        Ok(ExactTailDisposition::Prepared)
    } else {
        Err(AuthorityRuntimeError::CorruptTransitionDescriptor {
            detail: "disk state matches neither exact descriptor prior nor next state".to_string(),
        })
    }
}

fn classify_journal_tail(
    path: &Path,
    descriptor: &AuthorityTransitionDescriptorV1,
) -> Result<ExactTailDisposition, AuthorityRuntimeError> {
    let bytes = fs::read(path).map_err(|source| AuthorityRuntimeError::Io {
        operation: "read_journal_for_recovery",
        source,
    })?;
    let prior_len = usize::try_from(descriptor.core.prior_journal_length).map_err(|_| {
        AuthorityRuntimeError::CorruptTransitionDescriptor {
            detail: "prior journal length exceeds platform address space".to_string(),
        }
    })?;
    if bytes.len() < prior_len {
        return Err(AuthorityRuntimeError::CorruptTransitionDescriptor {
            detail: "journal is shorter than descriptor prior length".to_string(),
        });
    }
    let prior = &bytes[..prior_len];
    let (prior_sequence, prior_tail) = replay_journal(prior)?;
    let expected_prior_sequence = descriptor
        .core
        .prior_state
        .as_ref()
        .map_or(0, |state| state.core.journal_sequence);
    let expected_prior_tail = descriptor
        .core
        .prior_state
        .as_ref()
        .map(|state| state.core.journal_root_digest.as_str());
    if prior_sequence != expected_prior_sequence || prior_tail.as_deref() != expected_prior_tail {
        return Err(AuthorityRuntimeError::CorruptTransitionDescriptor {
            detail: "journal prefix does not match descriptor prior state".to_string(),
        });
    }
    if bytes.len() == prior_len {
        return Ok(ExactTailDisposition::Prior);
    }
    let encoded = descriptor.core.journal_record.encoded_line()?;
    if bytes[prior_len..] == encoded {
        Ok(ExactTailDisposition::Prepared)
    } else {
        Err(AuthorityRuntimeError::CorruptTransitionDescriptor {
            detail: "journal tail is not the exact descriptor-bound prepared record".to_string(),
        })
    }
}

fn classify_replay_tail(
    path: &Path,
    descriptor: &AuthorityTransitionDescriptorV1,
) -> Result<ExactTailDisposition, AuthorityRuntimeError> {
    let bytes = fs::read(path).map_err(|source| AuthorityRuntimeError::Io {
        operation: "read_replay_for_recovery",
        source,
    })?;
    let prior_len = usize::try_from(descriptor.core.prior_replay_length).map_err(|_| {
        AuthorityRuntimeError::CorruptTransitionDescriptor {
            detail: "prior replay length exceeds platform address space".to_string(),
        }
    })?;
    if bytes.len() < prior_len {
        return Err(AuthorityRuntimeError::CorruptTransitionDescriptor {
            detail: "replay is shorter than descriptor prior length".to_string(),
        });
    }
    let prior = &bytes[..prior_len];
    let (_, prior_sequence, prior_tail) = replay_authority_records(prior)?;
    let expected_prior_sequence = descriptor
        .core
        .prior_state
        .as_ref()
        .map_or(0, |state| state.core.replay_sequence);
    if prior_sequence != expected_prior_sequence
        || prior_tail != descriptor.core.prior_replay_tail_digest
    {
        return Err(AuthorityRuntimeError::CorruptTransitionDescriptor {
            detail: "replay prefix does not match descriptor prior state".to_string(),
        });
    }
    if bytes.len() == prior_len {
        return Ok(ExactTailDisposition::Prior);
    }
    let Some(record) = &descriptor.core.replay_record else {
        return Err(AuthorityRuntimeError::CorruptTransitionDescriptor {
            detail: "replay advanced but descriptor has no prepared replay record".to_string(),
        });
    };
    let encoded = record.encoded_line()?;
    if bytes[prior_len..] == encoded {
        Ok(ExactTailDisposition::Prepared)
    } else {
        Err(AuthorityRuntimeError::CorruptTransitionDescriptor {
            detail: "replay tail is not the exact descriptor-bound prepared record".to_string(),
        })
    }
}

fn truncate_exact_tail(
    path: &Path,
    prior_length: u64,
    operation: &'static str,
) -> Result<(), AuthorityRuntimeError> {
    refuse_symlink(path)?;
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|source| AuthorityRuntimeError::Io { operation, source })?;
    file.set_len(prior_length)
        .and_then(|_| file.sync_all())
        .map_err(|source| AuthorityRuntimeError::Io { operation, source })?;
    sync_parent(path)
}

fn remove_transition_descriptor(path: &Path) -> Result<(), AuthorityRuntimeError> {
    refuse_symlink(path)?;
    fs::remove_file(path).map_err(|source| AuthorityRuntimeError::Io {
        operation: "remove_transition_descriptor",
        source,
    })?;
    sync_parent(path)
}

struct PreparedTransitionExecutionV1<'a> {
    config: &'a AuthorityRuntimeConfig,
    catalog: &'a ActionCatalogV1,
    state_path: &'a Path,
    prior_state: Option<&'a AuthorityRuntimeStateV1>,
    next_state: &'a AuthorityRuntimeStateV1,
    journal_record: &'a AuthorityJournalRecordV1,
    journal: &'a mut AuthorityJournal,
    replay: &'a mut AuthorityReplayLedger,
    protected_backend: &'a mut dyn ProtectedEpochBackend,
    transition_fault: &'a mut Option<TransitionFaultPoint>,
}

fn execute_prepared_transition(
    input: PreparedTransitionExecutionV1<'_>,
) -> Result<(), AuthorityRuntimeError> {
    let PreparedTransitionExecutionV1 {
        config,
        catalog,
        state_path,
        prior_state,
        next_state,
        journal_record,
        journal,
        replay,
        protected_backend,
        transition_fault,
    } = input;
    let expected_prior = prior_state.map(AuthorityRuntimeStateV1::protected_snapshot);
    let observed = protected_backend
        .read_latest()
        .map_err(|detail| AuthorityRuntimeError::ProtectedEpoch { detail })?;
    if observed != expected_prior {
        replay.abort_pending();
        return Err(AuthorityRuntimeError::RollbackDetected {
            detail: "protected snapshot changed before descriptor preparation".to_string(),
        });
    }
    let descriptor_path = config.root.join(TRANSITION_DESCRIPTOR_FILE_NAME);
    refuse_symlink(&descriptor_path)?;
    if descriptor_path.exists() {
        replay.abort_pending();
        return Err(AuthorityRuntimeError::ConcurrentModification {
            detail: "a prepared transition descriptor already exists".to_string(),
        });
    }
    let descriptor = AuthorityTransitionDescriptorV1::new(
        prior_state.cloned(),
        next_state.clone(),
        journal_record.clone(),
        replay.pending_record().cloned(),
        journal.known_len,
        replay.known_len,
        replay.tail_digest.clone(),
    )?;
    descriptor.validate(config, catalog)?;
    atomic_replace_json(&descriptor_path, &descriptor)?;
    inject_transition_fault(transition_fault, TransitionFaultPoint::Descriptor)?;

    if descriptor.core.replay_record.is_some() {
        replay.append_pending()?;
    }
    inject_transition_fault(transition_fault, TransitionFaultPoint::Replay)?;
    journal.append_prepared(journal_record)?;
    inject_transition_fault(transition_fault, TransitionFaultPoint::Journal)?;
    protected_backend
        .compare_and_advance(expected_prior.as_ref(), &next_state.protected_snapshot())
        .map_err(|detail| AuthorityRuntimeError::ProtectedEpoch { detail })?;
    inject_transition_fault(transition_fault, TransitionFaultPoint::ProtectedCas)?;
    atomic_replace_json(state_path, next_state)?;
    inject_transition_fault(transition_fault, TransitionFaultPoint::State)?;
    remove_transition_descriptor(&descriptor_path)?;
    inject_transition_fault(transition_fault, TransitionFaultPoint::Cleanup)?;
    Ok(())
}

pub struct AuthorityRuntime {
    autonomy_admission_owner: Option<Arc<dyn AutonomyAdmissionOwner>>,
    /// Serializes every positive G2/G9 observation-admission-witness cycle.
    /// The G2 mutex is still released while G9 runs, avoiding lock inversion.
    autonomy_transaction_lock: Mutex<()>,
    inner: Mutex<AuthorityRuntimeInner>,
}

struct AuthorityRuntimeInner {
    lease: OwnerLease,
    config: AuthorityRuntimeConfig,
    state_path: PathBuf,
    state: AuthorityRuntimeStateV1,
    catalog: ActionCatalogV1,
    journal: AuthorityJournal,
    replay: AuthorityReplayLedger,
    sessions: AuthenticatedSessionRegistry,
    protected_backend: Box<dyn ProtectedEpochBackend>,
    positive_verifier: Box<dyn PositiveAuthorityVerifier>,
    service_identity_verifier: Option<Box<dyn ServiceIdentityVerifier>>,
    safety_verifier: Option<Box<dyn SafetyActuatorVerifier>>,
    transition_fault: Option<TransitionFaultPoint>,
    poisoned: bool,
}

impl AuthorityRuntime {
    pub fn bootstrap(
        mut config: AuthorityRuntimeConfig,
        protected_backend: Box<dyn ProtectedEpochBackend>,
    ) -> Result<Self, AuthorityRuntimeError> {
        Self::bootstrap_with_components(
            config,
            protected_backend,
            Box::new(ControlCryptoAuthorityVerifier),
            None,
            None,
            None,
        )
    }

    pub fn open(
        config: AuthorityRuntimeConfig,
        protected_backend: Box<dyn ProtectedEpochBackend>,
    ) -> Result<Self, AuthorityRuntimeError> {
        Self::open_with_components(
            config,
            protected_backend,
            Box::new(ControlCryptoAuthorityVerifier),
            None,
            None,
        )
    }

    fn bootstrap_with_components(
        mut config: AuthorityRuntimeConfig,
        mut protected_backend: Box<dyn ProtectedEpochBackend>,
        positive_verifier: Box<dyn PositiveAuthorityVerifier>,
        service_identity_verifier: Option<Box<dyn ServiceIdentityVerifier>>,
        safety_verifier: Option<Box<dyn SafetyActuatorVerifier>>,
        mut transition_fault: Option<TransitionFaultPoint>,
    ) -> Result<Self, AuthorityRuntimeError> {
        config.validate()?;
        let lease = OwnerLease::acquire(&config.root)?;
        config.root = lease.canonical_root().to_path_buf();
        lease.verify_root_binding()?;
        reject_orphan_state_temps(&config.root)?;
        let state_path = config.root.join(STATE_FILE_NAME);
        let journal_path = config.root.join(JOURNAL_FILE_NAME);
        let replay_path = config.root.join(REPLAY_FILE_NAME);
        let catalog = m1nd10_action_catalog()?;
        catalog.validate()?;
        recover_prepared_transition(&config, &catalog, protected_backend.as_mut())?;
        refuse_symlink(&state_path)?;
        if state_path.exists() {
            return Err(AuthorityRuntimeError::AlreadyBootstrapped { path: state_path });
        }
        if protected_backend
            .read_latest()
            .map_err(|detail| AuthorityRuntimeError::ProtectedEpoch { detail })?
            .is_some()
        {
            return Err(AuthorityRuntimeError::RollbackDetected {
                detail: "protected epoch exists while authoritative state is absent".to_string(),
            });
        }

        let mut replay = if replay_path.exists() {
            let ledger = AuthorityReplayLedger::open(replay_path.clone())?;
            if ledger.sequence != 0 || ledger.known_len != 0 {
                return Err(AuthorityRuntimeError::CorruptReplay {
                    detail: "descriptor-free bootstrap replay is not empty".to_string(),
                });
            }
            ledger
        } else {
            AuthorityReplayLedger::create(replay_path)?
        };
        let mut journal = if journal_path.exists() {
            let journal = AuthorityJournal::open(journal_path.clone())?;
            if journal.sequence != 0 || journal.known_len != 0 {
                return Err(AuthorityRuntimeError::CorruptJournal {
                    detail: "descriptor-free bootstrap journal is not empty".to_string(),
                });
            }
            journal
        } else {
            AuthorityJournal::create(journal_path)?
        };
        let bootstrap_record = journal.prepare(
            AuthorityJournalEventKind::BootstrapFrozen,
            catalog.catalog_digest.clone(),
            1,
            0,
        )?;
        let mut state = AuthorityRuntimeStateV1 {
            schema: AUTHORITY_RUNTIME_STATE_SCHEMA.to_string(),
            core: AuthorityRuntimeStateCoreV1 {
                organism_id: config.organism_id.clone(),
                repo_id: config.repo_id.clone(),
                brain_id: config.brain_id.clone(),
                audience: config.audience.clone(),
                revision: 0,
                active_mode: ActiveMode::HumanGated,
                activation_receipt_id: None,
                constitution_digest: config.constitution_digest.clone(),
                constitution_epoch: config.constitution_epoch,
                autonomy_epoch: 0,
                grants_digest: config.grants_digest.clone(),
                policy_registry_digest: config.policy_registry_digest.clone(),
                action_catalog_digest: catalog.catalog_digest.clone(),
                safety_kernel_digest: config.safety_kernel_digest.clone(),
                safety_actuator_identity_key_binary_policy_digest: config
                    .safety_actuator_identity_key_binary_policy_digest
                    .clone(),
                issuance_frozen: true,
                safety_state: SafetyState::Frozen,
                protected_epoch: 1,
                journal_sequence: bootstrap_record.core.sequence,
                journal_root_digest: bootstrap_record.record_digest.clone(),
                replay_sequence: 0,
                replay_root_digest: None,
                updated_at: 0,
            },
            record_digest: String::new(),
        };
        state.seal()?;
        state.validate(&config, &catalog)?;
        execute_prepared_transition(PreparedTransitionExecutionV1 {
            config: &config,
            catalog: &catalog,
            state_path: &state_path,
            prior_state: None,
            next_state: &state,
            journal_record: &bootstrap_record,
            journal: &mut journal,
            replay: &mut replay,
            protected_backend: protected_backend.as_mut(),
            transition_fault: &mut transition_fault,
        })?;
        lease.verify_root_binding()?;

        Ok(Self {
            autonomy_admission_owner: None,
            autonomy_transaction_lock: Mutex::new(()),
            inner: Mutex::new(AuthorityRuntimeInner {
                lease,
                config,
                state_path,
                state,
                catalog,
                journal,
                replay,
                sessions: AuthenticatedSessionRegistry::default(),
                protected_backend,
                positive_verifier,
                service_identity_verifier,
                safety_verifier,
                transition_fault,
                poisoned: false,
            }),
        })
    }

    fn open_with_components(
        mut config: AuthorityRuntimeConfig,
        mut protected_backend: Box<dyn ProtectedEpochBackend>,
        positive_verifier: Box<dyn PositiveAuthorityVerifier>,
        service_identity_verifier: Option<Box<dyn ServiceIdentityVerifier>>,
        safety_verifier: Option<Box<dyn SafetyActuatorVerifier>>,
    ) -> Result<Self, AuthorityRuntimeError> {
        config.validate()?;
        let lease = OwnerLease::acquire(&config.root)?;
        config.root = lease.canonical_root().to_path_buf();
        lease.verify_root_binding()?;
        reject_orphan_state_temps(&config.root)?;
        let state_path = config.root.join(STATE_FILE_NAME);
        let journal_path = config.root.join(JOURNAL_FILE_NAME);
        let replay_path = config.root.join(REPLAY_FILE_NAME);
        let catalog = m1nd10_action_catalog()?;
        catalog.validate()?;
        recover_prepared_transition(&config, &catalog, protected_backend.as_mut())?;
        for path in [&state_path, &journal_path, &replay_path] {
            refuse_symlink(path)?;
            if !path.exists() {
                return Err(AuthorityRuntimeError::MissingAuthorityRecord {
                    path: path.to_path_buf(),
                });
            }
        }
        let state = read_authority_state(&state_path)?;
        state.validate(&config, &catalog)?;
        let protected = protected_backend
            .read_latest()
            .map_err(|detail| AuthorityRuntimeError::ProtectedEpoch { detail })?;
        if protected.as_ref() != Some(&state.protected_snapshot()) {
            return Err(AuthorityRuntimeError::RollbackDetected {
                detail: format!(
                    "protected snapshot {:?} does not match authoritative state epoch {}",
                    protected, state.core.protected_epoch
                ),
            });
        }
        let journal = AuthorityJournal::open(journal_path)?;
        if journal.sequence != state.core.journal_sequence
            || journal.tail_digest.as_deref() != Some(state.core.journal_root_digest.as_str())
        {
            return Err(AuthorityRuntimeError::CorruptJournal {
                detail: "journal root does not match authoritative state".to_string(),
            });
        }
        let replay = AuthorityReplayLedger::open(replay_path)?;
        if replay.consumed_count() as u64 != state.core.replay_sequence
            || replay.tail_digest != state.core.replay_root_digest
        {
            return Err(AuthorityRuntimeError::RollbackDetected {
                detail: "replay ledger sequence/root does not match authoritative state"
                    .to_string(),
            });
        }
        lease.verify_root_binding()?;

        Ok(Self {
            autonomy_admission_owner: None,
            autonomy_transaction_lock: Mutex::new(()),
            inner: Mutex::new(AuthorityRuntimeInner {
                lease,
                config,
                state_path,
                state,
                catalog,
                journal,
                replay,
                sessions: AuthenticatedSessionRegistry::default(),
                protected_backend,
                positive_verifier,
                service_identity_verifier,
                safety_verifier,
                transition_fault: None,
                poisoned: false,
            }),
        })
    }

    #[cfg(all(test, unix))]
    fn bootstrap_software_test(
        config: AuthorityRuntimeConfig,
        protected_backend: SoftwareTestProtectedEpochBackend,
        safety_verifier: Option<Box<dyn SafetyActuatorVerifier>>,
    ) -> Result<Self, AuthorityRuntimeError> {
        Self::bootstrap_with_components(
            config,
            Box::new(protected_backend),
            Box::new(SoftwareTestPositiveAuthorityVerifier),
            None,
            safety_verifier,
            None,
        )
    }

    #[cfg(all(test, unix))]
    fn bootstrap_software_test_with_fault(
        config: AuthorityRuntimeConfig,
        protected_backend: SoftwareTestProtectedEpochBackend,
        safety_verifier: Option<Box<dyn SafetyActuatorVerifier>>,
        fault: TransitionFaultPoint,
    ) -> Result<Self, AuthorityRuntimeError> {
        Self::bootstrap_with_components(
            config,
            Box::new(protected_backend),
            Box::new(SoftwareTestPositiveAuthorityVerifier),
            None,
            safety_verifier,
            Some(fault),
        )
    }

    #[cfg(all(test, unix))]
    fn open_software_test(
        config: AuthorityRuntimeConfig,
        protected_backend: SoftwareTestProtectedEpochBackend,
        safety_verifier: Option<Box<dyn SafetyActuatorVerifier>>,
    ) -> Result<Self, AuthorityRuntimeError> {
        Self::open_with_components(
            config,
            Box::new(protected_backend),
            Box::new(SoftwareTestPositiveAuthorityVerifier),
            None,
            safety_verifier,
        )
    }

    #[cfg(all(test, unix))]
    fn set_transition_fault(&self, fault: TransitionFaultPoint) {
        self.inner.lock().transition_fault = Some(fault);
    }

    /// Install the sole constitutional autonomy owner before this runtime is
    /// shared.  There is intentionally no replacement API: swapping the G9
    /// owner underneath a live G2 runtime would create a second mode authority.
    pub fn install_autonomy_admission_owner(
        &mut self,
        owner: Arc<dyn AutonomyAdmissionOwner>,
    ) -> Result<(), AuthorityRuntimeError> {
        if self.autonomy_admission_owner.is_some() {
            return Err(AuthorityRuntimeError::AutonomyAdmissionOwnerAlreadyInstalled);
        }
        self.autonomy_admission_owner = Some(owner);
        Ok(())
    }

    /// Reconcile the protected G9 owner into G2 before any positive decision.
    /// G9 is read without the G2 lock; G2 then commits one journaled/protected
    /// mirror transition. Same-epoch drift, rollback, foreign scope, skipped
    /// epochs, or an unproven autonomous mode freezes all positive authority.
    pub fn synchronize_autonomy_authority(
        &self,
        now_ms: u64,
    ) -> Result<AuthorityRuntimeStateV1, AuthorityRuntimeError> {
        let owner = self
            .autonomy_admission_owner
            .as_ref()
            .ok_or(AuthorityRuntimeError::AutonomyAdmissionUnavailable)?;
        let projection = owner.read_projection(now_ms).map_err(|error| {
            AuthorityRuntimeError::AutonomyAdmission {
                detail: error.to_string(),
            }
        })?;
        self.inner
            .lock()
            .synchronize_autonomy_projection(&projection, now_ms)
    }

    pub fn status(&self) -> Result<AuthorityRuntimeStatusV1, AuthorityRuntimeError> {
        let inner = self.inner.lock();
        inner.ensure_live()?;
        Ok(AuthorityRuntimeStatusV1 {
            state: inner.state.clone(),
            protected_epoch_assurance: inner.protected_backend.assurance(),
            positive_verification_assurance: inner.positive_verifier.assurance(),
            semantic_catalog_entries: inner.catalog.entries.len(),
            transport_schema_parity_proven: false,
            multi_artifact_atomicity_proven: false,
            automatic_crash_recovery_proven: true,
        })
    }

    pub(crate) fn install_safety_verifier(
        &self,
        verifier: Box<dyn SafetyActuatorVerifier>,
    ) -> Result<(), AuthorityRuntimeError> {
        let mut inner = self.inner.lock();
        inner.ensure_live()?;
        inner.safety_verifier = Some(verifier);
        Ok(())
    }

    pub(crate) fn install_service_identity_verifier(
        &self,
        verifier: Box<dyn ServiceIdentityVerifier>,
    ) -> Result<(), AuthorityRuntimeError> {
        let mut inner = self.inner.lock();
        inner.ensure_live()?;
        inner.service_identity_verifier = Some(verifier);
        Ok(())
    }

    /// Read-only ceremony lookup used by the owner transport to bind an
    /// authentication attempt to the same trusted wire context before the
    /// one-shot challenge is consumed.
    pub(crate) fn pending_session_challenge(
        &self,
        challenge_id: &str,
        now_ms: u64,
    ) -> Result<Option<SessionChallengeV1>, AuthorityRuntimeError> {
        let mut inner = self.inner.lock();
        inner.ensure_live()?;
        let pending = inner.sessions.challenges.get(challenge_id).cloned();
        if pending
            .as_ref()
            .is_some_and(|pending| now_ms >= pending.challenge.core.expires_at)
        {
            inner.sessions.gc(now_ms);
            return Err(AuthorityRuntimeError::ChallengeExpired {
                challenge_id: challenge_id.to_string(),
            });
        }
        inner.sessions.gc(now_ms);
        Ok(pending
            .filter(|pending| !pending.consumed)
            .map(|pending| pending.challenge))
    }

    /// Read-only session lookup. It exposes no signing material; the transport
    /// uses it only to reject cross-wire reuse of an otherwise valid session.
    pub(crate) fn authenticated_session(
        &self,
        session_id: &str,
        now_ms: u64,
    ) -> Result<Option<AuthenticatedSessionV1>, AuthorityRuntimeError> {
        let mut inner = self.inner.lock();
        inner.ensure_live()?;
        let session = inner.sessions.sessions.get(session_id).cloned();
        inner.sessions.gc(now_ms);
        Ok(session.filter(|session| now_ms < session.expires_at))
    }

    pub(crate) fn issue_session_challenge(
        &self,
        request: SessionChallengeRequestV1,
        keys: &VerificationKeyRegistryV1,
        now_ms: u64,
    ) -> Result<SessionChallengeV1, AuthorityRuntimeError> {
        let mut inner = self.inner.lock();
        inner.ensure_live()?;
        inner.sessions.gc(now_ms);
        for (field, value) in [
            ("challenge_id", request.challenge_id.as_str()),
            ("subject_id", request.subject_id.as_str()),
            ("key_id", request.key_id.as_str()),
            ("app_host_identity", request.app_host_identity.as_str()),
            ("nonce", request.nonce.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        require_digest("session_context_digest", &request.session_context_digest)?;
        keys.resolve_active(
            &request.key_id,
            &request.subject_id,
            now_ms,
            inner.config.max_future_clock_skew_ms,
        )?;
        if inner.sessions.challenges.len() >= MAX_PENDING_SESSION_CHALLENGES {
            return Err(AuthorityRuntimeError::SessionRegistryCapacity {
                registry: "pending_challenges",
            });
        }
        if inner.sessions.issued_nonces.len() >= MAX_ISSUED_SESSION_NONCES {
            return Err(AuthorityRuntimeError::SessionRegistryCapacity {
                registry: "issued_nonces",
            });
        }
        if inner
            .sessions
            .challenges
            .contains_key(&request.challenge_id)
            || inner.sessions.issued_nonces.contains_key(&request.nonce)
        {
            return Err(AuthorityRuntimeError::DuplicateChallengeNonce);
        }
        inner
            .sessions
            .issued_nonces
            .insert(request.nonce.clone(), request.expires_at);
        let mut challenge = SessionChallengeV1 {
            schema: SESSION_CHALLENGE_SCHEMA.to_string(),
            core: SessionChallengeCoreV1 {
                challenge_id: request.challenge_id,
                subject_id: request.subject_id,
                key_id: request.key_id,
                app_host_identity: request.app_host_identity,
                audience: inner.config.audience.clone(),
                organism_id: inner.config.organism_id.clone(),
                brain_id: inner.config.brain_id.clone(),
                session_context_digest: request.session_context_digest,
                nonce: request.nonce,
                issued_at: request.issued_at,
                expires_at: request.expires_at,
            },
            challenge_digest: String::new(),
        };
        challenge.seal()?;
        if let Err(error) = challenge.validate(now_ms) {
            inner.sessions.issued_nonces.remove(&challenge.core.nonce);
            return Err(error);
        }
        inner.sessions.challenges.insert(
            challenge.core.challenge_id.clone(),
            PendingSessionChallenge {
                challenge: challenge.clone(),
                consumed: false,
            },
        );
        Ok(challenge)
    }

    pub(crate) fn authenticate_session(
        &self,
        challenge_id: &str,
        ingress: Ingress,
        capability: &AuthorityCapabilityV1,
        keys: &VerificationKeyRegistryV1,
        now_ms: u64,
    ) -> Result<AuthenticatedSessionV1, AuthorityRuntimeError> {
        let mut inner = self.inner.lock();
        inner.ensure_live()?;
        let challenge = inner
            .sessions
            .challenges
            .get(challenge_id)
            .ok_or_else(|| AuthorityRuntimeError::ChallengeNotFound {
                challenge_id: challenge_id.to_string(),
            })?
            .clone();
        inner.sessions.gc(now_ms);
        if challenge.consumed {
            return Err(AuthorityRuntimeError::ChallengeConsumed {
                challenge_id: challenge_id.to_string(),
            });
        }
        challenge.challenge.validate(now_ms)?;
        let action = ActionId::new("runtime.session.handshake").map_err(|error| {
            AuthorityRuntimeError::InvalidContract {
                detail: error.to_string(),
            }
        })?;
        let entry = inner.resolve_action(&action, ingress)?.clone();
        if entry.authority_floor != AuthorityFloor::Ordinary {
            return Err(AuthorityRuntimeError::UncoveredAuthorityFloor {
                action: action.to_string(),
                floor: entry.authority_floor,
            });
        }
        if capability.issuer_subject_id != challenge.challenge.core.subject_id
            || capability.issuer_key_id != challenge.challenge.core.key_id
            || capability.nonce != challenge.challenge.core.nonce
        {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "session_challenge_authority",
            });
        }
        mode_accepts(inner.state.core.active_mode, capability.authority_variant)?;
        let proof = inner.verify_positive(PositiveVerificationRequestV1 {
            capability,
            keys,
            expected_subject_id: &challenge.challenge.core.subject_id,
            expected_payload_digest: &challenge.challenge.challenge_digest,
            expected_action: action.as_str(),
            expected_mission_id: None,
            expected_mission_head_id: None,
            now_ms,
        })?;
        let session_id = digest_canonical(
            SESSION_ID_DIGEST_DOMAIN,
            &(
                challenge.challenge.challenge_digest.as_str(),
                proof.signed_body_digest.as_str(),
                keys.registry_epoch,
            ),
        )?;
        let session = AuthenticatedSessionV1 {
            session_id: session_id.clone(),
            subject_id: challenge.challenge.core.subject_id.clone(),
            key_id: challenge.challenge.core.key_id.clone(),
            app_host_identity: challenge.challenge.core.app_host_identity.clone(),
            audience: challenge.challenge.core.audience.clone(),
            session_context_digest: challenge.challenge.core.session_context_digest.clone(),
            key_registry_epoch: keys.registry_epoch,
            authenticated_at: now_ms,
            expires_at: challenge
                .challenge
                .core
                .expires_at
                .min(capability.expires_at),
            authentication_body_digest: proof.signed_body_digest.clone(),
            verification_assurance: inner.positive_verifier.assurance(),
        };
        if inner.sessions.sessions.len() >= MAX_AUTHENTICATED_SESSIONS {
            return Err(AuthorityRuntimeError::SessionRegistryCapacity {
                registry: "authenticated_sessions",
            });
        }
        inner.commit(
            AuthorityJournalEventKind::SessionAuthenticated,
            proof.signed_body_digest,
            now_ms,
            |_| {},
        )?;
        if let Some(pending) = inner.sessions.challenges.get_mut(challenge_id) {
            pending.consumed = true;
        }
        inner.sessions.sessions.insert(session_id, session.clone());
        Ok(session)
    }

    pub(crate) fn verify_bootstrap(
        &self,
        session_id: &str,
        session_context_digest: &str,
        ingress: Ingress,
        capability: &AuthorityCapabilityV1,
        keys: &VerificationKeyRegistryV1,
        now_ms: u64,
    ) -> Result<AuthorityRuntimeStateV1, AuthorityRuntimeError> {
        let mut inner = self.inner.lock();
        inner.ensure_live()?;
        if inner.state.core.active_mode != ActiveMode::HumanGated
            || !inner.state.core.issuance_frozen
            || inner.state.core.safety_state != SafetyState::Frozen
            || inner.state.core.activation_receipt_id.is_some()
        {
            return Err(AuthorityRuntimeError::ActivationConflict {
                detail: "bootstrap verification is only valid for the initial frozen record"
                    .to_string(),
            });
        }
        let session = inner.verify_session(session_id, session_context_digest, keys, now_ms)?;
        let action = ActionId::new("brain.bootstrap").map_err(|error| {
            AuthorityRuntimeError::InvalidContract {
                detail: error.to_string(),
            }
        })?;
        let entry = inner.resolve_action(&action, ingress)?;
        if entry.authority_floor != AuthorityFloor::PositiveSovereign {
            return Err(AuthorityRuntimeError::UncoveredAuthorityFloor {
                action: action.to_string(),
                floor: entry.authority_floor,
            });
        }
        if capability.authority_variant != AuthorityVariant::Human {
            return Err(AuthorityRuntimeError::AuthorityModeMismatch {
                mode: ActiveMode::HumanGated,
                variant: capability.authority_variant,
            });
        }
        let payload_digest = inner.state.record_digest.clone();
        let proof = inner.verify_positive(PositiveVerificationRequestV1 {
            capability,
            keys,
            expected_subject_id: &session.subject_id,
            expected_payload_digest: &payload_digest,
            expected_action: action.as_str(),
            expected_mission_id: None,
            expected_mission_head_id: None,
            now_ms,
        })?;
        inner.commit(
            AuthorityJournalEventKind::BootstrapVerified,
            proof.signed_body_digest,
            now_ms,
            |core| {
                core.issuance_frozen = false;
                core.safety_state = SafetyState::Healthy;
            },
        )?;
        Ok(inner.state.clone())
    }

    pub(crate) fn authorize_mutation(
        &self,
        request: AuthorityAuthorizationRequestV1,
        authority: AuthorityInputV1<'_>,
    ) -> Result<AuthorityAuthorizationReceiptV1, AuthorityRuntimeError> {
        let positive_authority = matches!(
            &authority,
            AuthorityInputV1::Positive { .. } | AuthorityInputV1::PositiveSovereign { .. }
        );
        let g9_witness_required = self.autonomy_admission_owner.is_some() && positive_authority;
        let _autonomy_transaction =
            g9_witness_required.then(|| self.autonomy_transaction_lock.lock());
        if g9_witness_required {
            self.synchronize_autonomy_authority(request.now_ms)?;
        }
        if let AuthorityInputV1::PositiveSovereign {
            capability,
            keys,
            metadata,
            autonomy_evidence,
        } = &authority
        {
            if metadata.capability_kind == CapabilityKind::Autonomy {
                let evidence = autonomy_evidence
                    .as_ref()
                    .ok_or(AuthorityRuntimeError::AutonomyAdmissionUnavailable)?;
                let now_ms = request.now_ms;
                let receipt = self
                    .authorize_autonomous_mutation(request, capability, keys, metadata, evidence)?;
                self.verify_post_authorization_autonomy_witness(&receipt, now_ms)?;
                return Ok(receipt);
            }
            if autonomy_evidence.is_some() {
                return Err(AuthorityRuntimeError::BindingMismatch {
                    field: "human_authority_forbids_autonomy_evidence",
                });
            }
        }

        let mut inner = self.inner.lock();
        inner.ensure_live()?;
        require_digest("authorization.payload_digest", &request.payload_digest)?;
        let entry = inner
            .resolve_action(&request.action, request.ingress)?
            .clone();
        if request.requested_effects != entry.complete_effects {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "complete_effects",
            });
        }
        let result = match (entry.authority_floor, authority) {
            (AuthorityFloor::SafetyOnly, AuthorityInputV1::Safety { attempt }) => {
                inner.authorize_safety(request, &entry, attempt)
            }
            (
                AuthorityFloor::SafetyOnly,
                AuthorityInputV1::Positive { .. }
                | AuthorityInputV1::PositiveSovereign { .. }
                | AuthorityInputV1::OrdinarySession { .. }
                | AuthorityInputV1::ServiceIdentity { .. },
            ) => Err(AuthorityRuntimeError::SafetyAuthorityRequired),
            (_, AuthorityInputV1::Safety { .. }) => {
                Err(AuthorityRuntimeError::PositiveAuthorityRequired)
            }
            (AuthorityFloor::Ordinary, AuthorityInputV1::OrdinarySession { keys, role }) => {
                inner.authorize_ordinary_session(request, &entry, keys, role)
            }
            (AuthorityFloor::ServiceIdentity, AuthorityInputV1::ServiceIdentity { assertion }) => {
                inner.authorize_service_identity(request, &entry, assertion)
            }
            (
                AuthorityFloor::ScopedGrantA2 | AuthorityFloor::PositiveSovereign,
                AuthorityInputV1::PositiveSovereign {
                    capability,
                    keys,
                    metadata,
                    autonomy_evidence: _,
                },
            ) => inner.authorize_positive(request, &entry, capability, keys, metadata, None),
            (
                AuthorityFloor::ScopedGrantA2 | AuthorityFloor::PositiveSovereign,
                AuthorityInputV1::Positive { capability, keys },
            ) => {
                if capability.authority_variant != AuthorityVariant::Human {
                    return Err(AuthorityRuntimeError::AutonomyAdmissionUnavailable);
                }
                let metadata = PositiveSovereignAuthorityMetadataV1 {
                    role: Role::Author,
                    capability_kind: CapabilityKind::Human,
                    authority_decision_digest: capability.signed_body_digest()?,
                    applicable_grant_id: None,
                    applicable_tier: None,
                };
                inner.authorize_positive(request, &entry, capability, keys, &metadata, None)
            }
            (
                floor,
                AuthorityInputV1::OrdinarySession { .. }
                | AuthorityInputV1::Positive { .. }
                | AuthorityInputV1::PositiveSovereign { .. }
                | AuthorityInputV1::ServiceIdentity { .. },
            ) => Err(AuthorityRuntimeError::UncoveredAuthorityFloor {
                action: entry.action.to_string(),
                floor,
            }),
        };
        drop(inner);
        let receipt = result?;
        if g9_witness_required {
            self.verify_post_authorization_autonomy_witness(&receipt, receipt.core.authorized_at)?;
        }
        Ok(receipt)
    }

    fn verify_post_authorization_autonomy_witness(
        &self,
        receipt: &AuthorityAuthorizationReceiptV1,
        now_ms: u64,
    ) -> Result<(), AuthorityRuntimeError> {
        let owner = self
            .autonomy_admission_owner
            .as_ref()
            .ok_or(AuthorityRuntimeError::AutonomyAdmissionUnavailable)?;
        let projection = match owner.read_projection(now_ms) {
            Ok(projection) => projection,
            Err(error) => {
                let marker = digest_canonical(
                    "m1nd-autonomy-post-authorization-read-failure-v1",
                    &error.to_string(),
                )?;
                self.inner.lock().freeze_autonomy_integrity(
                    "post_authorization_owner_unavailable",
                    &marker,
                    &marker,
                    now_ms,
                )?;
                return Err(AuthorityRuntimeError::AutonomyAdmission {
                    detail: error.to_string(),
                });
            }
        };
        let mut inner = self.inner.lock();
        inner.synchronize_autonomy_projection(&projection, now_ms)?;
        if matches!(
            receipt.core.authority,
            AuthorizationAuthorityV1::Autonomous { .. }
        ) {
            let mismatch = [
                (
                    "post_authorization_state_digest",
                    receipt.core.autonomy_committed_state_digest.as_deref()
                        != Some(projection.state_digest.as_str()),
                ),
                (
                    "post_authorization_protected_root_digest",
                    receipt.core.autonomy_protected_root_digest.as_deref()
                        != Some(projection.protected_root_digest.as_str()),
                ),
            ]
            .into_iter()
            .find_map(|(field, differs)| differs.then_some(field));
            if let Some(field) = mismatch {
                inner.freeze_autonomy_integrity(
                    field,
                    &projection.state_digest,
                    &projection.protected_root_digest,
                    now_ms,
                )?;
                return Err(AuthorityRuntimeError::AutonomyMirrorMismatch { field });
            }
        }
        Ok(())
    }

    fn authorize_autonomous_mutation(
        &self,
        request: AuthorityAuthorizationRequestV1,
        capability: &AuthorityCapabilityV1,
        keys: &VerificationKeyRegistryV1,
        metadata: &PositiveSovereignAuthorityMetadataV1,
        evidence: &AutonomyAuthorityEvidenceV1,
    ) -> Result<AuthorityAuthorizationReceiptV1, AuthorityRuntimeError> {
        let owner = self
            .autonomy_admission_owner
            .as_ref()
            .ok_or(AuthorityRuntimeError::AutonomyAdmissionUnavailable)?;

        // Cheap and cryptographic fail-fast pass. Nothing here consumes G2 or
        // G9 replay state, and the full checks run again after G9 admission.
        {
            let mut inner = self.inner.lock();
            inner.ensure_live()?;
            require_digest("authorization.payload_digest", &request.payload_digest)?;
            if inner.state.core.issuance_frozen {
                return Err(AuthorityRuntimeError::IssuanceFrozen);
            }
            let entry = inner
                .resolve_action(&request.action, request.ingress)?
                .clone();
            if !matches!(
                entry.authority_floor,
                AuthorityFloor::ScopedGrantA2 | AuthorityFloor::PositiveSovereign
            ) || request.requested_effects != entry.complete_effects
            {
                return Err(AuthorityRuntimeError::BindingMismatch {
                    field: "autonomy_action_or_effects",
                });
            }
            let session_id = request
                .session_id
                .as_deref()
                .ok_or(AuthorityRuntimeError::PositiveAuthorityRequired)?;
            let session_context_digest = request
                .session_context_digest
                .as_deref()
                .ok_or(AuthorityRuntimeError::SessionContextMismatch)?;
            let session =
                inner.verify_session(session_id, session_context_digest, keys, request.now_ms)?;
            inner.resolve_exact_policy(
                &request,
                &entry,
                &session.subject_id,
                capability.authority_variant,
                metadata.applicable_grant_id.as_deref(),
                metadata.applicable_tier,
            )?;
            inner.precheck_positive(PositiveVerificationRequestV1 {
                capability,
                keys,
                expected_subject_id: &session.subject_id,
                expected_payload_digest: &request.payload_digest,
                expected_action: request.action.as_str(),
                expected_mission_id: request.mission_id.as_deref(),
                expected_mission_head_id: request.mission_head_id.as_deref(),
                now_ms: request.now_ms,
            })?;
            validate_autonomy_authority_binding(
                evidence,
                ExpectedAutonomyAuthorityBindingV1 {
                    generic_capability: capability,
                    target_action: request.action.as_str(),
                    payload_digest: &request.payload_digest,
                    subject_id: &session.subject_id,
                    organism_id: &inner.config.organism_id,
                    repo_id: &inner.config.repo_id,
                    brain_id: &inner.config.brain_id,
                    mission_id: request.mission_id.as_deref(),
                    mission_head_id: request.mission_head_id.as_deref(),
                    authority_decision_digest: &metadata.authority_decision_digest,
                    applicable_grant_id: metadata.applicable_grant_id.as_deref(),
                    applicable_tier: metadata.applicable_tier,
                },
            )
            .map_err(|error| AuthorityRuntimeError::AutonomyAdmission {
                detail: error.to_string(),
            })?;
        }

        let admission = owner.admit(evidence, request.now_ms).map_err(|error| {
            AuthorityRuntimeError::AutonomyAdmission {
                detail: error.to_string(),
            }
        })?;

        let mut inner = self.inner.lock();
        inner.ensure_live()?;
        let entry = inner
            .resolve_action(&request.action, request.ingress)?
            .clone();
        if request.requested_effects != entry.complete_effects {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "complete_effects",
            });
        }
        inner.authorize_positive(
            request,
            &entry,
            capability,
            keys,
            metadata,
            Some(&admission),
        )
    }
}

impl AuthorityRuntimeInner {
    fn synchronize_autonomy_projection(
        &mut self,
        projection: &AutonomyManifestProjectionV1,
        now_ms: u64,
    ) -> Result<AuthorityRuntimeStateV1, AuthorityRuntimeError> {
        self.ensure_live()?;
        projection
            .validate()
            .map_err(|error| AuthorityRuntimeError::AutonomyAdmission {
                detail: error.to_string(),
            })?;

        let target_mode =
            parse_active_mode_wire(&projection.autonomy.active_mode).ok_or_else(|| {
                AuthorityRuntimeError::AutonomyAdmission {
                    detail: "G9 projection carries an unknown active mode".to_string(),
                }
            })?;
        let target_safety = parse_safety_state_wire(&projection.autonomy.sentinel_safety_state)
            .ok_or_else(|| AuthorityRuntimeError::AutonomyAdmission {
                detail: "G9 projection carries an unknown safety state".to_string(),
            })?;

        let mismatch = if projection.organism_id != self.config.organism_id {
            Some("organism_id")
        } else if projection.repo_id != self.config.repo_id {
            Some("repo_id")
        } else if projection.brain_id != self.config.brain_id {
            Some("brain_id")
        } else if projection.autonomy.safety_kernel_digest != self.state.core.safety_kernel_digest {
            Some("safety_kernel_digest")
        } else if projection.autonomy.autonomy_epoch < self.state.core.autonomy_epoch {
            Some("autonomy_epoch_rollback")
        } else if projection.autonomy.autonomy_epoch == self.state.core.autonomy_epoch {
            [
                ("active_mode", target_mode != self.state.core.active_mode),
                (
                    "activation_receipt_id",
                    projection.autonomy.activation_receipt_id
                        != self
                            .state
                            .core
                            .activation_receipt_id
                            .as_deref()
                            .unwrap_or_default(),
                ),
                (
                    "constitution_digest",
                    projection.autonomy.constitution_digest != self.state.core.constitution_digest,
                ),
                (
                    "constitution_epoch",
                    projection.autonomy.constitution_epoch != self.state.core.constitution_epoch,
                ),
                (
                    "grants_digest",
                    projection.autonomy.grants_digest != self.state.core.grants_digest,
                ),
                (
                    "issuance_frozen",
                    projection.autonomy.issuance_frozen != self.state.core.issuance_frozen,
                ),
                (
                    "safety_state",
                    target_safety != self.state.core.safety_state,
                ),
            ]
            .into_iter()
            .find_map(|(field, differs)| differs.then_some(field))
        } else {
            None
        };

        if let Some(field) = mismatch {
            self.freeze_autonomy_integrity(
                field,
                &projection.state_digest,
                &projection.protected_root_digest,
                now_ms,
            )?;
            return Err(AuthorityRuntimeError::AutonomyMirrorMismatch { field });
        }
        if projection.autonomy.autonomy_epoch == self.state.core.autonomy_epoch {
            return Ok(self.state.clone());
        }

        let invalid_transition = self.state.core.revision == 0
            || self.state.core.issuance_frozen
            || projection.autonomy.autonomy_epoch
                != self.state.core.autonomy_epoch.saturating_add(1)
            || projection.autonomy.constitution_epoch < self.state.core.constitution_epoch
            || !autonomy_mode_transition_allowed(
                self.state.core.active_mode,
                target_mode,
                target_safety,
            )
            || (target_mode != ActiveMode::HumanGated
                && (projection.autonomy.activation_receipt_id.trim().is_empty()
                    || !projection
                        .autonomy
                        .mechanically_proven_modes
                        .contains(active_mode_wire(target_mode))))
            || matches!(target_safety, SafetyState::Healthy) == projection.autonomy.issuance_frozen;
        if invalid_transition {
            let field = "autonomy_transition";
            self.freeze_autonomy_integrity(
                field,
                &projection.state_digest,
                &projection.protected_root_digest,
                now_ms,
            )?;
            return Err(AuthorityRuntimeError::AutonomyMirrorMismatch { field });
        }

        let payload_digest =
            digest_canonical("m1nd-autonomy-authority-synchronization-v1", projection)?;
        self.commit(
            AuthorityJournalEventKind::AutonomyAuthoritySynchronized,
            payload_digest,
            now_ms,
            |core| {
                core.active_mode = target_mode;
                core.activation_receipt_id = if projection.autonomy.activation_receipt_id.is_empty()
                {
                    None
                } else {
                    Some(projection.autonomy.activation_receipt_id.clone())
                };
                core.constitution_digest = projection.autonomy.constitution_digest.clone();
                core.constitution_epoch = projection.autonomy.constitution_epoch;
                core.autonomy_epoch = projection.autonomy.autonomy_epoch;
                core.grants_digest = projection.autonomy.grants_digest.clone();
                core.issuance_frozen = projection.autonomy.issuance_frozen;
                core.safety_state = target_safety;
            },
        )?;
        Ok(self.state.clone())
    }

    fn freeze_autonomy_integrity(
        &mut self,
        field: &'static str,
        state_digest: &str,
        protected_root_digest: &str,
        now_ms: u64,
    ) -> Result<(), AuthorityRuntimeError> {
        let payload_digest = digest_canonical(
            "m1nd-autonomy-mirror-mismatch-v1",
            &(field, state_digest, protected_root_digest),
        )?;
        self.commit(
            AuthorityJournalEventKind::AutonomyMirrorMismatchFrozen,
            payload_digest,
            now_ms,
            |core| {
                core.issuance_frozen = true;
                core.safety_state = SafetyState::Frozen;
            },
        )
    }

    fn ensure_live(&self) -> Result<(), AuthorityRuntimeError> {
        // This closes operations that begin after a same-UID rename/recreate of
        // the authority root. It deliberately does not overclaim elimination
        // of the sub-operation path race; protected storage or UID/sandbox
        // separation remains required for full same-UID assurance.
        self.lease.verify_root_binding()?;
        if self.poisoned || self.journal.poisoned || self.replay.is_poisoned() {
            return Err(AuthorityRuntimeError::Poisoned);
        }
        Ok(())
    }

    fn resolve_action(
        &self,
        action: &ActionId,
        ingress: Ingress,
    ) -> Result<&ActionCatalogEntryV1, AuthorityRuntimeError> {
        let entry = self
            .catalog
            .entries
            .binary_search_by(|entry| entry.action.cmp(action))
            .ok()
            .and_then(|index| self.catalog.entries.get(index))
            .ok_or_else(|| AuthorityRuntimeError::UnknownAction {
                action: action.to_string(),
            })?;
        if !entry.ingresses.contains(&ingress) {
            return Err(AuthorityRuntimeError::UnreachableIngress {
                action: action.to_string(),
                ingress,
            });
        }
        if entry.complete_effects.is_empty() {
            return Err(AuthorityRuntimeError::InvalidContract {
                detail: format!("action {action} has no complete effect coverage"),
            });
        }
        Ok(entry)
    }

    fn resolve_exact_policy(
        &self,
        request: &AuthorityAuthorizationRequestV1,
        entry: &ActionCatalogEntryV1,
        subject_id: &str,
        authority_variant: AuthorityVariant,
        applicable_grant_id: Option<&str>,
        applicable_tier: Option<AutonomyTier>,
    ) -> Result<ReachablePolicyTupleV1, AuthorityRuntimeError> {
        let tuple = ReachablePolicyTupleV1 {
            ingress: request.ingress,
            action: request.action.clone(),
            active_mode: self.state.core.active_mode,
            subject_id: subject_id.to_string(),
            authority_variant,
            applicable_grant_id: applicable_grant_id.map(str::to_string),
            applicable_tier,
            risk_class: entry.risk_class,
        };
        let rule = self
            .config
            .policy_registry
            .rules
            .iter()
            .find(|rule| rule.tuple == tuple)
            .ok_or(AuthorityRuntimeError::BindingMismatch {
                field: "exact_policy_tuple",
            })?;
        if rule.effects != entry.complete_effects
            || request.requested_effects != rule.effects
            || self.config.policy_registry.policy_digest != self.state.core.policy_registry_digest
        {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "exact_policy_rule",
            });
        }
        Ok(tuple)
    }

    fn verify_session(
        &mut self,
        session_id: &str,
        session_context_digest: &str,
        keys: &VerificationKeyRegistryV1,
        now_ms: u64,
    ) -> Result<AuthenticatedSessionV1, AuthorityRuntimeError> {
        let session = self
            .sessions
            .sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| AuthorityRuntimeError::SessionNotFound {
                session_id: session_id.to_string(),
            })?;
        self.sessions.gc(now_ms);
        if now_ms >= session.expires_at {
            return Err(AuthorityRuntimeError::SessionExpired {
                session_id: session_id.to_string(),
            });
        }
        if session.session_context_digest != session_context_digest {
            return Err(AuthorityRuntimeError::SessionContextMismatch);
        }
        if keys.registry_epoch < session.key_registry_epoch {
            return Err(AuthorityRuntimeError::SessionKeyInactive {
                detail: "verification key registry epoch moved backwards".to_string(),
            });
        }
        keys.resolve_active(
            &session.key_id,
            &session.subject_id,
            now_ms,
            self.config.max_future_clock_skew_ms,
        )
        .map_err(|error| AuthorityRuntimeError::SessionKeyInactive {
            detail: error.to_string(),
        })?;
        Ok(session)
    }

    fn verify_positive(
        &mut self,
        request: PositiveVerificationRequestV1<'_>,
    ) -> Result<PositiveAuthorityProofV1, AuthorityRuntimeError> {
        let PositiveVerificationRequestV1 {
            capability,
            keys,
            expected_subject_id,
            expected_payload_digest,
            expected_action,
            expected_mission_id,
            expected_mission_head_id,
            now_ms,
        } = request;
        mode_accepts(self.state.core.active_mode, capability.authority_variant)?;
        if capability.policy_registry_digest != self.state.core.policy_registry_digest {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "policy_registry_digest",
            });
        }
        if capability.constitution_digest != self.state.core.constitution_digest {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "constitution_digest",
            });
        }
        let context = CapabilityVerificationContext {
            now_ms,
            max_future_clock_skew_ms: self.config.max_future_clock_skew_ms,
            expected_schema: AUTHORITY_CAPABILITY_SCHEMA,
            expected_audience: &self.config.audience,
            expected_subject_id,
            expected_payload_digest,
            expected_organism_id: &self.config.organism_id,
            expected_brain_id: &self.config.brain_id,
            expected_mission_id,
            expected_mission_head_id,
            expected_action,
            expected_authority_variant: capability.authority_variant,
            expected_active_mode: self.state.core.active_mode,
        };
        let proof =
            match self
                .positive_verifier
                .verify_once(capability, keys, context, &mut self.replay)
            {
                Ok(proof) => proof,
                Err(error) => {
                    self.replay.abort_pending();
                    return Err(error);
                }
            };
        let capability_body_digest = match capability.signed_body_digest() {
            Ok(digest) => digest,
            Err(error) => {
                self.replay.abort_pending();
                return Err(error.into());
            }
        };
        if proof.subject_id != expected_subject_id
            || proof.key_id != capability.issuer_key_id
            || proof.signed_body_digest != capability_body_digest
        {
            self.replay.abort_pending();
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "verified_positive_authority",
            });
        }
        Ok(proof)
    }

    /// Cryptographic/binding precheck without replay consumption or authority
    /// commit. Autonomous admission calls this before burning the protected G9
    /// capability, then `verify_positive` repeats it and consumes replay after
    /// the G9 receipt exists.
    fn precheck_positive(
        &self,
        request: PositiveVerificationRequestV1<'_>,
    ) -> Result<(), AuthorityRuntimeError> {
        let PositiveVerificationRequestV1 {
            capability,
            keys,
            expected_subject_id,
            expected_payload_digest,
            expected_action,
            expected_mission_id,
            expected_mission_head_id,
            now_ms,
        } = request;
        mode_accepts(self.state.core.active_mode, capability.authority_variant)?;
        if capability.policy_registry_digest != self.state.core.policy_registry_digest {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "policy_registry_digest",
            });
        }
        if capability.constitution_digest != self.state.core.constitution_digest {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "constitution_digest",
            });
        }
        self.positive_verifier.precheck(
            capability,
            keys,
            CapabilityVerificationContext {
                now_ms,
                max_future_clock_skew_ms: self.config.max_future_clock_skew_ms,
                expected_schema: AUTHORITY_CAPABILITY_SCHEMA,
                expected_audience: &self.config.audience,
                expected_subject_id,
                expected_payload_digest,
                expected_organism_id: &self.config.organism_id,
                expected_brain_id: &self.config.brain_id,
                expected_mission_id,
                expected_mission_head_id,
                expected_action,
                expected_authority_variant: capability.authority_variant,
                expected_active_mode: self.state.core.active_mode,
            },
        )
    }

    fn authorize_ordinary_session(
        &mut self,
        request: AuthorityAuthorizationRequestV1,
        entry: &ActionCatalogEntryV1,
        keys: &VerificationKeyRegistryV1,
        role: Role,
    ) -> Result<AuthorityAuthorizationReceiptV1, AuthorityRuntimeError> {
        require_non_empty("transport_session_id", &request.transport_session_id)?;
        require_digest("ingress_context_digest", &request.ingress_context_digest)?;
        let session_id = request
            .session_id
            .as_deref()
            .ok_or(AuthorityRuntimeError::PositiveAuthorityRequired)?;
        let session_context_digest = request
            .session_context_digest
            .as_deref()
            .ok_or(AuthorityRuntimeError::SessionContextMismatch)?;
        let session =
            self.verify_session(session_id, session_context_digest, keys, request.now_ms)?;
        let exact_policy = self.resolve_exact_policy(
            &request,
            entry,
            &session.subject_id,
            AuthorityVariant::Ordinary,
            None,
            None,
        )?;
        let authority_body_digest = digest_canonical(
            "m1nd-ordinary-session-authorization-v1",
            &(
                session.authentication_body_digest.as_str(),
                request.action.as_str(),
                request.payload_digest.as_str(),
                request.ingress,
                role,
            ),
        )?;
        self.commit(
            AuthorityJournalEventKind::PositiveMutationAuthorized,
            authority_body_digest.clone(),
            request.now_ms,
            |_| {},
        )?;
        AuthorityAuthorizationReceiptV1::new(AuthorityAuthorizationReceiptCoreV1 {
            organism_id: self.config.organism_id.clone(),
            repo_id: self.config.repo_id.clone(),
            brain_id: self.config.brain_id.clone(),
            subject_id: session.subject_id,
            role,
            capability_id: format!("session:{}", session.session_id),
            capability_kind: None,
            verified_object_digest: request.payload_digest,
            mission_id: request.mission_id,
            mission_head_id: request.mission_head_id,
            transport_session_id: request.transport_session_id,
            ingress_context_digest: request.ingress_context_digest,
            action: entry.action.clone(),
            ingress: request.ingress,
            complete_effects: entry.complete_effects.clone(),
            active_mode: self.state.core.active_mode,
            constitution_digest: self.state.core.constitution_digest.clone(),
            constitution_epoch: self.state.core.constitution_epoch,
            autonomy_epoch: self.state.core.autonomy_epoch,
            protected_epoch_at_decision: self.state.core.protected_epoch,
            policy_registry_digest: self.state.core.policy_registry_digest.clone(),
            exact_policy_tuple: exact_policy,
            authority_decision_digest: None,
            autonomy_admission_receipt_digest: None,
            autonomy_committed_state_digest: None,
            autonomy_protected_root_digest: None,
            authority: AuthorizationAuthorityV1::OrdinarySession {
                assurance: session.verification_assurance,
            },
            authority_body_digest,
            replay_sequence: self.state.core.replay_sequence,
            journal_sequence: self.state.core.journal_sequence,
            journal_root_digest: self.state.core.journal_root_digest.clone(),
            protected_epoch: self.state.core.protected_epoch,
            authorized_at: request.now_ms,
            expires_at: session.expires_at,
        })
    }

    fn authorize_positive(
        &mut self,
        request: AuthorityAuthorizationRequestV1,
        entry: &ActionCatalogEntryV1,
        capability: &AuthorityCapabilityV1,
        keys: &VerificationKeyRegistryV1,
        metadata: &PositiveSovereignAuthorityMetadataV1,
        autonomy_admission: Option<&AutonomyAdmissionOutcomeV1>,
    ) -> Result<AuthorityAuthorizationReceiptV1, AuthorityRuntimeError> {
        require_non_empty("transport_session_id", &request.transport_session_id)?;
        require_digest("ingress_context_digest", &request.ingress_context_digest)?;
        if self.state.core.issuance_frozen {
            return Err(AuthorityRuntimeError::IssuanceFrozen);
        }
        let session_id = request
            .session_id
            .as_deref()
            .ok_or(AuthorityRuntimeError::PositiveAuthorityRequired)?;
        let session_context_digest = request
            .session_context_digest
            .as_deref()
            .ok_or(AuthorityRuntimeError::SessionContextMismatch)?;
        let session =
            self.verify_session(session_id, session_context_digest, keys, request.now_ms)?;
        require_digest(
            "authority_decision_digest",
            &metadata.authority_decision_digest,
        )?;
        let expected_kind = if capability.authority_variant == AuthorityVariant::Human {
            CapabilityKind::Human
        } else {
            CapabilityKind::Autonomy
        };
        if metadata.capability_kind != expected_kind {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "capability_kind",
            });
        }
        match metadata.capability_kind {
            CapabilityKind::Autonomy => {
                let admission = autonomy_admission
                    .ok_or(AuthorityRuntimeError::AutonomyAdmissionUnavailable)?;
                self.require_autonomy_mirror(
                    admission,
                    capability.authority_variant,
                    &metadata.authority_decision_digest,
                    request.now_ms,
                )?;
            }
            CapabilityKind::Human if autonomy_admission.is_some() => {
                return Err(AuthorityRuntimeError::BindingMismatch {
                    field: "human_authority_forbids_autonomy_admission",
                });
            }
            CapabilityKind::Human => {}
            _ => {
                return Err(AuthorityRuntimeError::BindingMismatch {
                    field: "positive_capability_kind",
                });
            }
        }
        let exact_policy = self.resolve_exact_policy(
            &request,
            entry,
            &session.subject_id,
            capability.authority_variant,
            metadata.applicable_grant_id.as_deref(),
            metadata.applicable_tier,
        )?;
        let proof = self.verify_positive(PositiveVerificationRequestV1 {
            capability,
            keys,
            expected_subject_id: &session.subject_id,
            expected_payload_digest: &request.payload_digest,
            expected_action: request.action.as_str(),
            expected_mission_id: request.mission_id.as_deref(),
            expected_mission_head_id: request.mission_head_id.as_deref(),
            now_ms: request.now_ms,
        })?;
        let assurance = self.positive_verifier.assurance();
        let autonomy_receipt = autonomy_admission.map(|admission| &admission.receipt);
        self.commit(
            AuthorityJournalEventKind::PositiveMutationAuthorized,
            proof.signed_body_digest.clone(),
            request.now_ms,
            |_| {},
        )?;
        AuthorityAuthorizationReceiptV1::new(AuthorityAuthorizationReceiptCoreV1 {
            organism_id: self.config.organism_id.clone(),
            repo_id: self.config.repo_id.clone(),
            brain_id: self.config.brain_id.clone(),
            subject_id: session.subject_id,
            role: metadata.role,
            capability_id: capability.capability_id.clone(),
            capability_kind: Some(metadata.capability_kind),
            verified_object_digest: request.payload_digest,
            mission_id: request.mission_id,
            mission_head_id: request.mission_head_id,
            transport_session_id: request.transport_session_id,
            ingress_context_digest: request.ingress_context_digest,
            action: entry.action.clone(),
            ingress: request.ingress,
            complete_effects: entry.complete_effects.clone(),
            active_mode: self.state.core.active_mode,
            constitution_digest: self.state.core.constitution_digest.clone(),
            constitution_epoch: self.state.core.constitution_epoch,
            autonomy_epoch: self.state.core.autonomy_epoch,
            protected_epoch_at_decision: self.state.core.protected_epoch,
            policy_registry_digest: self.state.core.policy_registry_digest.clone(),
            exact_policy_tuple: exact_policy,
            authority_decision_digest: Some(metadata.authority_decision_digest.clone()),
            autonomy_admission_receipt_digest: autonomy_receipt
                .map(|receipt| receipt.receipt_digest.clone()),
            autonomy_committed_state_digest: autonomy_receipt
                .map(|receipt| receipt.committed_state_digest.clone()),
            autonomy_protected_root_digest: autonomy_receipt
                .map(|receipt| receipt.protected_root_digest.clone()),
            authority: if let Some(receipt) = autonomy_receipt {
                AuthorizationAuthorityV1::Autonomous {
                    variant: capability.authority_variant,
                    capability_assurance: assurance,
                    admission_receipt_digest: receipt.receipt_digest.clone(),
                }
            } else {
                AuthorizationAuthorityV1::Positive {
                    variant: capability.authority_variant,
                    assurance,
                }
            },
            authority_body_digest: proof.signed_body_digest,
            replay_sequence: proof.replay.sequence,
            journal_sequence: self.state.core.journal_sequence,
            journal_root_digest: self.state.core.journal_root_digest.clone(),
            protected_epoch: self.state.core.protected_epoch,
            authorized_at: request.now_ms,
            expires_at: capability.expires_at.min(session.expires_at),
        })
    }

    fn require_autonomy_mirror(
        &mut self,
        admission: &AutonomyAdmissionOutcomeV1,
        authority_variant: AuthorityVariant,
        authority_decision_digest: &str,
        now_ms: u64,
    ) -> Result<(), AuthorityRuntimeError> {
        let autonomy = &admission.projection.autonomy;
        let expected_activation = self
            .state
            .core
            .activation_receipt_id
            .as_deref()
            .unwrap_or_default();
        let mismatch = [
            (
                "active_mode",
                autonomy.active_mode != active_mode_wire(self.state.core.active_mode),
            ),
            (
                "activation_receipt_id",
                autonomy.activation_receipt_id != expected_activation,
            ),
            (
                "constitution_digest",
                autonomy.constitution_digest != self.state.core.constitution_digest,
            ),
            (
                "constitution_epoch",
                autonomy.constitution_epoch != self.state.core.constitution_epoch,
            ),
            (
                "autonomy_epoch",
                autonomy.autonomy_epoch != self.state.core.autonomy_epoch,
            ),
            (
                "grants_digest",
                autonomy.grants_digest != self.state.core.grants_digest,
            ),
            (
                "issuance_frozen",
                autonomy.issuance_frozen != self.state.core.issuance_frozen,
            ),
            (
                "safety_state",
                autonomy.sentinel_safety_state != safety_state_wire(self.state.core.safety_state),
            ),
            (
                "authority_variant",
                admission.receipt.authority_variant != authority_variant,
            ),
            (
                "authority_decision_digest",
                admission.receipt.decision_digest != authority_decision_digest,
            ),
            (
                "admission_state_digest",
                admission.receipt.committed_state_digest != admission.projection.state_digest,
            ),
            (
                "admission_protected_root_digest",
                admission.receipt.protected_root_digest
                    != admission.projection.protected_root_digest,
            ),
        ]
        .into_iter()
        .find_map(|(field, differs)| differs.then_some(field));

        if let Some(field) = mismatch {
            let payload_digest = digest_canonical(
                "m1nd-autonomy-mirror-mismatch-v1",
                &(
                    field,
                    admission.receipt.receipt_digest.as_str(),
                    admission.projection.state_digest.as_str(),
                    admission.projection.protected_root_digest.as_str(),
                ),
            )?;
            self.commit(
                AuthorityJournalEventKind::AutonomyMirrorMismatchFrozen,
                payload_digest,
                now_ms,
                |core| {
                    // A G2/G9 split is an organism-wide authority-integrity
                    // failure. The existing global fence intentionally blocks
                    // Human positive authority too until explicit recovery.
                    core.issuance_frozen = true;
                    core.safety_state = SafetyState::Frozen;
                },
            )?;
            return Err(AuthorityRuntimeError::AutonomyMirrorMismatch { field });
        }
        Ok(())
    }

    fn authorize_service_identity(
        &mut self,
        request: AuthorityAuthorizationRequestV1,
        entry: &ActionCatalogEntryV1,
        assertion: &ServiceIdentityAssertionV1,
    ) -> Result<AuthorityAuthorizationReceiptV1, AuthorityRuntimeError> {
        if assertion.schema != SERVICE_IDENTITY_ASSERTION_SCHEMA
            || assertion.core.action != request.action
            || assertion.core.object_digest != request.payload_digest
            || assertion.core.mission_id != request.mission_id
            || assertion.core.mission_head_id != request.mission_head_id
            || assertion.core.organism_id != self.config.organism_id
            || assertion.core.brain_id != self.config.brain_id
            || assertion.core.transport_session_id != request.transport_session_id
            || assertion.core.ingress_context_digest != request.ingress_context_digest
            || assertion.core.audience != self.config.audience
            || assertion.core.issued_at > request.now_ms
            || request.now_ms >= assertion.core.expires_at
            || assertion.signature.is_empty()
        {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "service_identity_assertion",
            });
        }
        require_digest("service.object_digest", &assertion.core.object_digest)?;
        require_digest(
            "service.ingress_context_digest",
            &assertion.core.ingress_context_digest,
        )?;
        let pinned = self
            .config
            .service_identities
            .get(&assertion.core.service_id)
            .ok_or(AuthorityRuntimeError::BindingMismatch {
                field: "pinned_service_identity",
            })?
            .clone();
        if pinned.subject_id != assertion.core.subject_id
            || pinned.key_id != assertion.core.key_id
            || pinned.role != assertion.core.role
            || pinned.identity_key_binary_policy_digest
                != assertion.core.identity_key_binary_policy_digest
            || !pinned.allowed_actions.contains(&request.action)
            || pinned
                .expires_at
                .is_some_and(|expiry| request.now_ms >= expiry)
        {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "pinned_service_identity",
            });
        }
        let verifier = self
            .service_identity_verifier
            .as_mut()
            .ok_or(AuthorityRuntimeError::ServiceIdentityVerifierUnavailable)?;
        let verified = verifier
            .verify(assertion, &pinned)
            .map_err(|detail| AuthorityRuntimeError::ServiceIdentityVerification { detail })?;
        let signed_body_digest = assertion.signed_body_digest()?;
        if verified.signed_body_digest != signed_body_digest {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "verified_service_identity",
            });
        }
        let exact_policy = self.resolve_exact_policy(
            &request,
            entry,
            &pinned.subject_id,
            AuthorityVariant::Ordinary,
            None,
            None,
        )?;
        let replay = self.replay.consume(
            &ReplayClaimV1 {
                schema: REPLAY_CLAIM_SCHEMA.to_string(),
                namespace: "pinned-service-identity".to_string(),
                issuer_subject_id: pinned.subject_id.clone(),
                key_id: pinned.key_id.clone(),
                subject_id: pinned.subject_id.clone(),
                nonce: assertion.core.nonce.clone(),
                object_digest: signed_body_digest.clone(),
                issued_at: assertion.core.issued_at,
                expires_at: assertion.core.expires_at,
            },
            request.now_ms,
            self.config.max_future_clock_skew_ms,
        )?;
        self.commit(
            AuthorityJournalEventKind::PositiveMutationAuthorized,
            signed_body_digest.clone(),
            request.now_ms,
            |_| {},
        )?;
        AuthorityAuthorizationReceiptV1::new(AuthorityAuthorizationReceiptCoreV1 {
            organism_id: self.config.organism_id.clone(),
            repo_id: self.config.repo_id.clone(),
            brain_id: self.config.brain_id.clone(),
            subject_id: pinned.subject_id,
            role: pinned.role,
            capability_id: format!("service:{}:{}", pinned.service_id, assertion.core.nonce),
            capability_kind: None,
            verified_object_digest: request.payload_digest,
            mission_id: request.mission_id,
            mission_head_id: request.mission_head_id,
            transport_session_id: assertion.core.transport_session_id.clone(),
            ingress_context_digest: assertion.core.ingress_context_digest.clone(),
            action: entry.action.clone(),
            ingress: request.ingress,
            complete_effects: entry.complete_effects.clone(),
            active_mode: self.state.core.active_mode,
            constitution_digest: self.state.core.constitution_digest.clone(),
            constitution_epoch: self.state.core.constitution_epoch,
            autonomy_epoch: self.state.core.autonomy_epoch,
            protected_epoch_at_decision: self.state.core.protected_epoch,
            policy_registry_digest: self.state.core.policy_registry_digest.clone(),
            exact_policy_tuple: exact_policy,
            authority_decision_digest: None,
            autonomy_admission_receipt_digest: None,
            autonomy_committed_state_digest: None,
            autonomy_protected_root_digest: None,
            authority: AuthorizationAuthorityV1::ServiceIdentity {
                service_id: pinned.service_id,
                assurance: verified.assurance,
            },
            authority_body_digest: signed_body_digest,
            replay_sequence: replay.sequence,
            journal_sequence: self.state.core.journal_sequence,
            journal_root_digest: self.state.core.journal_root_digest.clone(),
            protected_epoch: self.state.core.protected_epoch,
            authorized_at: request.now_ms,
            expires_at: assertion.core.expires_at,
        })
    }

    fn authorize_safety(
        &mut self,
        request: AuthorityAuthorizationRequestV1,
        entry: &ActionCatalogEntryV1,
        attempt: &SafetyActuatorAttemptV1,
    ) -> Result<AuthorityAuthorizationReceiptV1, AuthorityRuntimeError> {
        require_non_empty("transport_session_id", &request.transport_session_id)?;
        require_digest("ingress_context_digest", &request.ingress_context_digest)?;
        attempt.validate_structural(request.now_ms)?;
        if attempt.core.action != request.action
            || attempt.core.payload_digest != request.payload_digest
            || attempt.core.negative_effects != entry.complete_effects
            || attempt.core.constitution_epoch != self.state.core.constitution_epoch
            || attempt.core.autonomy_epoch != self.state.core.autonomy_epoch
            || attempt.core.actuator_identity_key_binary_policy_digest
                != self
                    .state
                    .core
                    .safety_actuator_identity_key_binary_policy_digest
        {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "safety_actuator_attempt",
            });
        }
        let exact_policy = self.resolve_exact_policy(
            &request,
            entry,
            &attempt.core.actuator_subject_id,
            AuthorityVariant::SafetyKernel,
            None,
            None,
        )?;
        let verifier = self
            .safety_verifier
            .as_mut()
            .ok_or(AuthorityRuntimeError::SafetyVerifierUnavailable)?;
        let verified = verifier
            .verify(attempt)
            .map_err(|detail| AuthorityRuntimeError::SafetyVerification { detail })?;
        let signed_body_digest = attempt.signed_body_digest()?;
        if verified.signed_body_digest != signed_body_digest
            || verified.key_id != attempt.core.actuator_key_id
            || verified.subject_id != attempt.core.actuator_subject_id
        {
            return Err(AuthorityRuntimeError::BindingMismatch {
                field: "verified_safety_actuator",
            });
        }
        let replay = self.replay.consume(
            &ReplayClaimV1 {
                schema: REPLAY_CLAIM_SCHEMA.to_string(),
                namespace: "safety-actuator".to_string(),
                issuer_subject_id: attempt.core.actuator_subject_id.clone(),
                key_id: attempt.core.actuator_key_id.clone(),
                subject_id: attempt.core.actuator_subject_id.clone(),
                nonce: attempt.core.nonce.clone(),
                object_digest: signed_body_digest.clone(),
                issued_at: attempt.core.issued_at,
                expires_at: attempt.core.expires_at,
            },
            request.now_ms,
            self.config.max_future_clock_skew_ms,
        )?;
        self.commit(
            AuthorityJournalEventKind::SafetyMutationAuthorized,
            signed_body_digest.clone(),
            request.now_ms,
            |_| {},
        )?;
        AuthorityAuthorizationReceiptV1::new(AuthorityAuthorizationReceiptCoreV1 {
            organism_id: self.config.organism_id.clone(),
            repo_id: self.config.repo_id.clone(),
            brain_id: self.config.brain_id.clone(),
            subject_id: attempt.core.actuator_subject_id.clone(),
            role: Role::MissionService,
            capability_id: attempt.core.attempt_id.clone(),
            capability_kind: Some(CapabilityKind::Safety),
            verified_object_digest: request.payload_digest,
            mission_id: request.mission_id,
            mission_head_id: request.mission_head_id,
            transport_session_id: request.transport_session_id,
            ingress_context_digest: request.ingress_context_digest,
            action: entry.action.clone(),
            ingress: request.ingress,
            complete_effects: entry.complete_effects.clone(),
            active_mode: self.state.core.active_mode,
            constitution_digest: self.state.core.constitution_digest.clone(),
            constitution_epoch: self.state.core.constitution_epoch,
            autonomy_epoch: self.state.core.autonomy_epoch,
            protected_epoch_at_decision: self.state.core.protected_epoch,
            policy_registry_digest: self.state.core.policy_registry_digest.clone(),
            exact_policy_tuple: exact_policy,
            authority_decision_digest: None,
            autonomy_admission_receipt_digest: None,
            autonomy_committed_state_digest: None,
            autonomy_protected_root_digest: None,
            authority: AuthorizationAuthorityV1::SafetyActuator {
                assurance: verified.assurance,
            },
            authority_body_digest: signed_body_digest,
            replay_sequence: replay.sequence,
            journal_sequence: self.state.core.journal_sequence,
            journal_root_digest: self.state.core.journal_root_digest.clone(),
            protected_epoch: self.state.core.protected_epoch,
            authorized_at: request.now_ms,
            expires_at: attempt.core.expires_at,
        })
    }

    fn commit(
        &mut self,
        event_kind: AuthorityJournalEventKind,
        payload_digest: String,
        now_ms: u64,
        mutate: impl FnOnce(&mut AuthorityRuntimeStateCoreV1),
    ) -> Result<(), AuthorityRuntimeError> {
        self.ensure_live()?;
        if let Err(error) = require_digest("journal.payload_digest", &payload_digest) {
            self.replay.abort_pending();
            self.poisoned = true;
            return Err(error);
        }
        let staged_replay = self
            .replay
            .pending_record()
            .map(|record| (record.sequence, record.record_digest.clone()));
        let next_epoch = self.state.core.protected_epoch.saturating_add(1);
        let journal_record =
            match self
                .journal
                .prepare(event_kind, payload_digest, next_epoch, now_ms)
            {
                Ok(record) => record,
                Err(error) => {
                    self.replay.abort_pending();
                    self.poisoned = true;
                    return Err(error);
                }
            };
        let mut next = self.state.clone();
        next.core.revision = next.core.revision.saturating_add(1);
        next.core.protected_epoch = next_epoch;
        next.core.journal_sequence = journal_record.core.sequence;
        next.core.journal_root_digest = journal_record.record_digest.clone();
        if let Some((staged_replay_sequence, staged_replay_root)) = staged_replay {
            next.core.replay_sequence = staged_replay_sequence;
            next.core.replay_root_digest = Some(staged_replay_root);
        }
        next.core.updated_at = now_ms;
        mutate(&mut next.core);
        if let Err(error) = next
            .seal()
            .map_err(AuthorityRuntimeError::from)
            .and_then(|_| next.validate(&self.config, &self.catalog))
        {
            self.replay.abort_pending();
            self.poisoned = true;
            return Err(error);
        }
        if let Err(error) = execute_prepared_transition(PreparedTransitionExecutionV1 {
            config: &self.config,
            catalog: &self.catalog,
            state_path: &self.state_path,
            prior_state: Some(&self.state),
            next_state: &next,
            journal_record: &journal_record,
            journal: &mut self.journal,
            replay: &mut self.replay,
            protected_backend: self.protected_backend.as_mut(),
            transition_fault: &mut self.transition_fault,
        }) {
            self.poisoned = true;
            return Err(error);
        }
        self.state = next;
        Ok(())
    }
}

fn mode_accepts(mode: ActiveMode, variant: AuthorityVariant) -> Result<(), AuthorityRuntimeError> {
    let accepted = match mode {
        ActiveMode::HumanGated => variant == AuthorityVariant::Human,
        ActiveMode::PolicyAutonomous => {
            matches!(variant, AuthorityVariant::Human | AuthorityVariant::Policy)
        }
        ActiveMode::FullAutonomy => variant == AuthorityVariant::AgentQuorum,
    };
    if accepted {
        Ok(())
    } else {
        Err(AuthorityRuntimeError::AuthorityModeMismatch { mode, variant })
    }
}

const fn active_mode_wire(mode: ActiveMode) -> &'static str {
    match mode {
        ActiveMode::HumanGated => "HUMAN_GATED",
        ActiveMode::PolicyAutonomous => "POLICY_AUTONOMOUS",
        ActiveMode::FullAutonomy => "FULL_AUTONOMY",
    }
}

fn parse_active_mode_wire(value: &str) -> Option<ActiveMode> {
    match value {
        "HUMAN_GATED" => Some(ActiveMode::HumanGated),
        "POLICY_AUTONOMOUS" => Some(ActiveMode::PolicyAutonomous),
        "FULL_AUTONOMY" => Some(ActiveMode::FullAutonomy),
        _ => None,
    }
}

fn parse_safety_state_wire(value: &str) -> Option<SafetyState> {
    match value {
        "HEALTHY" => Some(SafetyState::Healthy),
        "FROZEN" => Some(SafetyState::Frozen),
        "PENDING_RED" => Some(SafetyState::PendingRed),
        "RECOVERING" => Some(SafetyState::Recovering),
        _ => None,
    }
}

fn autonomy_mode_transition_allowed(
    current: ActiveMode,
    target: ActiveMode,
    target_safety: SafetyState,
) -> bool {
    current == target
        || matches!(
            (current, target),
            (ActiveMode::HumanGated, ActiveMode::PolicyAutonomous)
                | (ActiveMode::PolicyAutonomous, ActiveMode::FullAutonomy)
        )
        || (target == ActiveMode::HumanGated && target_safety != SafetyState::Healthy)
}

const fn safety_state_wire(state: SafetyState) -> &'static str {
    match state {
        SafetyState::Healthy => "HEALTHY",
        SafetyState::Frozen => "FROZEN",
        SafetyState::PendingRed => "PENDING_RED",
        SafetyState::Recovering => "RECOVERING",
    }
}

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(1);

fn atomic_replace_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AuthorityRuntimeError> {
    refuse_symlink(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| AuthorityRuntimeError::InvalidContract {
            detail: "authority state path has no parent".to_string(),
        })?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AuthorityRuntimeError::InvalidContract {
            detail: "authority state path is not UTF-8".to_string(),
        })?;
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_path = parent.join(format!(
        ".{file_name}.tmp.{}.{}",
        std::process::id(),
        counter
    ));
    refuse_symlink(&temp_path)?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "create_authority_state_temp",
                source,
            })?;
        let bytes = canonical_json(value)?;
        file.write_all(&bytes)
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "write_authority_state_temp",
                source,
            })?;
        file.sync_all()
            .map_err(|source| AuthorityRuntimeError::Io {
                operation: "sync_authority_state_temp",
                source,
            })?;
        refuse_symlink(path)?;
        fs::rename(&temp_path, path).map_err(|source| AuthorityRuntimeError::Io {
            operation: "rename_authority_state",
            source,
        })?;
        sync_parent(path)
    })();
    if result.is_err() && temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn read_authority_state(path: &Path) -> Result<AuthorityRuntimeStateV1, AuthorityRuntimeError> {
    refuse_symlink(path)?;
    let metadata = fs::metadata(path).map_err(|source| AuthorityRuntimeError::Io {
        operation: "metadata_authority_state",
        source,
    })?;
    if !metadata.is_file() {
        return Err(AuthorityRuntimeError::CorruptState {
            detail: "authoritative state is not a regular file".to_string(),
        });
    }
    let bytes = fs::read(path).map_err(|source| AuthorityRuntimeError::Io {
        operation: "read_authority_state",
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|error| AuthorityRuntimeError::CorruptState {
        detail: format!("authoritative state JSON is invalid: {error}"),
    })
}

fn reject_orphan_state_temps(root: &Path) -> Result<(), AuthorityRuntimeError> {
    let prefixes = [
        format!(".{STATE_FILE_NAME}.tmp."),
        format!(".{TRANSITION_DESCRIPTOR_FILE_NAME}.tmp."),
    ];
    for entry in fs::read_dir(root).map_err(|source| AuthorityRuntimeError::Io {
        operation: "scan_authority_state_temps",
        source,
    })? {
        let entry = entry.map_err(|source| AuthorityRuntimeError::Io {
            operation: "read_authority_state_temp_entry",
            source,
        })?;
        if prefixes
            .iter()
            .any(|prefix| entry.file_name().to_string_lossy().starts_with(prefix))
        {
            return Err(AuthorityRuntimeError::CorruptState {
                detail: format!(
                    "orphan atomic state temporary requires operator recovery: {}",
                    entry.path().display()
                ),
            });
        }
    }
    Ok(())
}

fn refuse_symlink(path: &Path) -> Result<(), AuthorityRuntimeError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(AuthorityRuntimeError::InvalidContract {
                detail: format!("authority runtime refuses symbolic link {}", path.display()),
            })
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(AuthorityRuntimeError::Io {
            operation: "inspect_authority_path",
            source,
        }),
    }
}

fn sync_parent(path: &Path) -> Result<(), AuthorityRuntimeError> {
    let parent = path
        .parent()
        .ok_or_else(|| AuthorityRuntimeError::InvalidContract {
            detail: "authority runtime path has no parent".to_string(),
        })?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| AuthorityRuntimeError::Io {
            operation: "sync_authority_parent_directory",
            source,
        })
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), AuthorityRuntimeError> {
    if value.trim().is_empty() {
        return Err(AuthorityRuntimeError::InvalidContract {
            detail: format!("required field '{field}' is empty"),
        });
    }
    Ok(())
}

fn require_digest(field: &'static str, value: &str) -> Result<(), AuthorityRuntimeError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AuthorityRuntimeError::InvalidContract {
            detail: format!("required digest '{field}' is not 64 hexadecimal characters"),
        });
    }
    Ok(())
}

#[cfg(all(test, unix))]
struct SoftwareTestPositiveAuthorityVerifier;

#[cfg(all(test, unix))]
impl PositiveAuthorityVerifier for SoftwareTestPositiveAuthorityVerifier {
    fn assurance(&self) -> AuthorityVerificationAssurance {
        AuthorityVerificationAssurance::SoftwareTestOnlyNotProven
    }

    fn precheck(
        &self,
        capability: &AuthorityCapabilityV1,
        keys: &VerificationKeyRegistryV1,
        context: CapabilityVerificationContext<'_>,
    ) -> Result<(), AuthorityRuntimeError> {
        validate_software_test_positive(capability, keys, context)
    }

    fn verify_once(
        &mut self,
        capability: &AuthorityCapabilityV1,
        keys: &VerificationKeyRegistryV1,
        context: CapabilityVerificationContext<'_>,
        replay: &mut dyn ReplayLedger,
    ) -> Result<PositiveAuthorityProofV1, AuthorityRuntimeError> {
        validate_software_test_positive(capability, keys, context)?;
        let signed_body_digest = capability.signed_body_digest()?;
        let replay = replay.consume(
            &ReplayClaimV1 {
                schema: REPLAY_CLAIM_SCHEMA.to_string(),
                namespace: "authority-capability".to_string(),
                issuer_subject_id: capability.issuer_subject_id.clone(),
                key_id: capability.issuer_key_id.clone(),
                subject_id: capability.subject_id.clone(),
                nonce: capability.nonce.clone(),
                object_digest: signed_body_digest.clone(),
                issued_at: capability.issued_at,
                expires_at: capability.expires_at,
            },
            context.now_ms,
            context.max_future_clock_skew_ms,
        )?;
        Ok(PositiveAuthorityProofV1 {
            signed_body_digest,
            key_id: capability.issuer_key_id.clone(),
            subject_id: capability.subject_id.clone(),
            replay,
        })
    }
}

#[cfg(all(test, unix))]
fn validate_software_test_positive(
    capability: &AuthorityCapabilityV1,
    keys: &VerificationKeyRegistryV1,
    context: CapabilityVerificationContext<'_>,
) -> Result<(), AuthorityRuntimeError> {
    if capability.schema != context.expected_schema
        || capability.audience != context.expected_audience
        || capability.subject_id != context.expected_subject_id
        || capability.payload_digest != context.expected_payload_digest
        || capability.organism_id != context.expected_organism_id
        || capability.brain_id != context.expected_brain_id
        || capability.mission_id.as_deref() != context.expected_mission_id
        || capability.mission_head_id.as_deref() != context.expected_mission_head_id
        || capability.action.as_str() != context.expected_action
        || capability.authority_variant != context.expected_authority_variant
        || capability.active_mode != context.expected_active_mode
        || capability.key_registry_epoch != keys.registry_epoch
        || !capability.authority_variant.is_positive_sovereign()
        || capability.signature.is_empty()
        || capability.expires_at <= capability.issued_at
    {
        return Err(AuthorityRuntimeError::BindingMismatch {
            field: "software_test_positive_authority",
        });
    }
    let key = keys.resolve_active(
        &capability.issuer_key_id,
        &capability.issuer_subject_id,
        context.now_ms,
        context.max_future_clock_skew_ms,
    )?;
    if key.algorithm != capability.algorithm {
        return Err(AuthorityRuntimeError::BindingMismatch {
            field: "software_test_positive_authority.algorithm",
        });
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::{Arc, Barrier};
    use std::thread;

    use ed25519_dalek::{Signer as _, SigningKey};
    use m1nd_control::{
        sign_capability, ActionEffectFloorV1, ActionPolicyRuleV1, AuthoritySigner,
        AuthoritySignerError, AuthorityStatus, IdentityStatus, VerificationKeyV1,
        ACTION_POLICY_REGISTRY_SCHEMA, ED25519_ALGORITHM, VERIFICATION_KEY_REGISTRY_SCHEMA,
    };
    use tempfile::TempDir;

    use super::*;

    const NOW: u64 = 100;
    const VALID_TEST_PUBLIC_KEY: &str =
        "5866666666666666666666666666666666666666666666666666666666666666";

    fn test_digest(label: &str) -> String {
        digest_canonical("m1nd-authority-runtime-test-v1", &label).unwrap()
    }

    fn test_config(root: &Path) -> AuthorityRuntimeConfig {
        let catalog = m1nd10_action_catalog().unwrap();
        let mut tuples = Vec::new();
        let mut rules = Vec::new();
        let mut floors = Vec::new();
        for entry in &catalog.entries {
            if entry.authority_floor == AuthorityFloor::ScopedGrantA2 {
                continue;
            }
            let (subject_id, authority_variant) = match entry.authority_floor {
                AuthorityFloor::Ordinary => ("owner-1", AuthorityVariant::Ordinary),
                AuthorityFloor::PositiveSovereign => ("owner-1", AuthorityVariant::Human),
                AuthorityFloor::ServiceIdentity => ("runner-service-1", AuthorityVariant::Ordinary),
                AuthorityFloor::SafetyOnly => ("safety-actuator-1", AuthorityVariant::SafetyKernel),
                AuthorityFloor::ScopedGrantA2 => unreachable!(),
            };
            floors.push(ActionEffectFloorV1 {
                action: entry.action.clone(),
                required_effects: entry.complete_effects.clone(),
            });
            for ingress in &entry.ingresses {
                let tuple = ReachablePolicyTupleV1 {
                    ingress: *ingress,
                    action: entry.action.clone(),
                    active_mode: ActiveMode::HumanGated,
                    subject_id: subject_id.to_string(),
                    authority_variant,
                    applicable_grant_id: None,
                    applicable_tier: None,
                    risk_class: entry.risk_class,
                };
                tuples.push(tuple.clone());
                rules.push(ActionPolicyRuleV1 {
                    tuple,
                    effects: entry.complete_effects.clone(),
                });
            }
        }
        let mut policy_registry = ActionPolicyRegistryV1 {
            schema: ACTION_POLICY_REGISTRY_SCHEMA.to_string(),
            policy_version: "software-test-policy-v1".to_string(),
            reachable_tuples: tuples,
            rules,
            action_effect_floors: floors,
            policy_digest: String::new(),
        };
        policy_registry.seal().unwrap();
        AuthorityRuntimeConfig {
            root: root.to_path_buf(),
            organism_id: "organism-1".to_string(),
            repo_id: "repo-1".to_string(),
            brain_id: "brain-1".to_string(),
            audience: "m1nd-runtime".to_string(),
            constitution_digest: test_digest("constitution"),
            constitution_epoch: 7,
            grants_digest: test_digest("grants"),
            policy_registry_digest: policy_registry.policy_digest.clone(),
            policy_registry,
            service_identities: BTreeMap::from([(
                "runnerd-1".to_string(),
                PinnedServiceIdentityV1 {
                    service_id: "runnerd-1".to_string(),
                    subject_id: "runner-service-1".to_string(),
                    key_id: "runner-key-1".to_string(),
                    role: Role::Runner,
                    organism_id: "organism-1".to_string(),
                    brain_id: "brain-1".to_string(),
                    audience: "m1nd-runtime".to_string(),
                    identity_key_binary_policy_digest: test_digest("runner-pin"),
                    allowed_actions: BTreeSet::from([
                        ActionId::new("mission.service.execution_started").unwrap(),
                        ActionId::new("mission.service.execution_terminal").unwrap(),
                    ]),
                    expires_at: None,
                },
            )]),
            safety_kernel_digest: test_digest("safety-kernel"),
            safety_actuator_identity_key_binary_policy_digest: test_digest("actuator-pin"),
            max_future_clock_skew_ms: 10,
        }
    }

    fn test_keys() -> VerificationKeyRegistryV1 {
        VerificationKeyRegistryV1 {
            schema: VERIFICATION_KEY_REGISTRY_SCHEMA.to_string(),
            registry_epoch: 3,
            keys: BTreeMap::from([(
                "owner-key-1".to_string(),
                VerificationKeyV1 {
                    key_id: "owner-key-1".to_string(),
                    subject_id: "owner-1".to_string(),
                    algorithm: ED25519_ALGORITHM.to_string(),
                    public_key: VALID_TEST_PUBLIC_KEY.to_string(),
                    created_at: 1,
                    activated_at: 2,
                    expires_at: None,
                    revoked_at: None,
                    rotated_at: None,
                    replacement_key_id: None,
                    status: IdentityStatus::Active,
                },
            )]),
        }
    }

    struct RealFixtureEd25519Signer {
        signing_key: SigningKey,
    }

    impl RealFixtureEd25519Signer {
        fn deterministic() -> Self {
            Self {
                signing_key: SigningKey::from_bytes(&[7u8; 32]),
            }
        }
    }

    impl AuthoritySigner for RealFixtureEd25519Signer {
        fn key_id(&self) -> &str {
            "owner-key-1"
        }

        fn subject_id(&self) -> &str {
            "owner-1"
        }

        fn algorithm(&self) -> &str {
            ED25519_ALGORITHM
        }

        fn public_key_bytes(&self) -> Result<Vec<u8>, AuthoritySignerError> {
            Ok(self.signing_key.verifying_key().to_bytes().to_vec())
        }

        fn sign(&self, message: &[u8]) -> Result<Vec<u8>, AuthoritySignerError> {
            Ok(self.signing_key.sign(message).to_bytes().to_vec())
        }
    }

    fn hex_bytes(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn real_fixture_keys(signer: &RealFixtureEd25519Signer) -> VerificationKeyRegistryV1 {
        let mut keys = test_keys();
        keys.keys.get_mut("owner-key-1").unwrap().public_key =
            hex_bytes(&signer.signing_key.verifying_key().to_bytes());
        keys
    }

    fn cryptographically_sign_test_capability(
        capability: &mut AuthorityCapabilityV1,
        keys: &VerificationKeyRegistryV1,
        signer: &RealFixtureEd25519Signer,
    ) {
        cryptographically_sign_test_capability_at(capability, keys, signer, NOW);
    }

    fn cryptographically_sign_test_capability_at(
        capability: &mut AuthorityCapabilityV1,
        keys: &VerificationKeyRegistryV1,
        signer: &RealFixtureEd25519Signer,
        now_ms: u64,
    ) {
        sign_capability(capability, keys, signer, now_ms, 10).unwrap();
    }

    #[cfg(feature = "serve")]
    async fn rest_authority_json<T: serde::Serialize>(
        router: &axum::Router,
        path: &str,
        transport_session_id: &str,
        body: &T,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        use axum::body::{to_bytes, Body};
        use axum::http::Request;
        use tower::ServiceExt;

        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("content-type", "application/json")
                    .header("m1nd-transport-session-id", transport_session_id)
                    .header("m1nd-caller-root", "/workspace/m1nd")
                    .body(Body::from(serde_json::to_vec(body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        (status, value)
    }

    #[allow(clippy::too_many_arguments)]
    fn test_capability(
        config: &AuthorityRuntimeConfig,
        action: &str,
        payload_digest: String,
        nonce: &str,
        variant: AuthorityVariant,
        mode: ActiveMode,
        mission_id: Option<&str>,
        mission_head_id: Option<&str>,
    ) -> AuthorityCapabilityV1 {
        AuthorityCapabilityV1 {
            schema: AUTHORITY_CAPABILITY_SCHEMA.to_string(),
            capability_id: format!("capability-{nonce}"),
            issuer_subject_id: "owner-1".to_string(),
            issuer_key_id: "owner-key-1".to_string(),
            algorithm: ED25519_ALGORITHM.to_string(),
            subject_id: "owner-1".to_string(),
            audience: config.audience.clone(),
            organism_id: config.organism_id.clone(),
            brain_id: config.brain_id.clone(),
            mission_id: mission_id.map(str::to_string),
            mission_head_id: mission_head_id.map(str::to_string),
            action: ActionId::new(action).unwrap(),
            authority_variant: variant,
            active_mode: mode,
            payload_digest,
            policy_registry_digest: config.policy_registry_digest.clone(),
            constitution_digest: config.constitution_digest.clone(),
            key_registry_epoch: 3,
            issued_at: NOW - 10,
            expires_at: NOW + 1_000,
            nonce: nonce.to_string(),
            signature: OpaqueSignature::new("software-test-signature-not-cryptographic"),
        }
    }

    fn authenticate_test_session(
        runtime: &AuthorityRuntime,
        config: &AuthorityRuntimeConfig,
        keys: &VerificationKeyRegistryV1,
        suffix: &str,
    ) -> AuthenticatedSessionV1 {
        let context_digest = test_digest(&format!("session-context-{suffix}"));
        let nonce = format!("session-nonce-{suffix}");
        let challenge = runtime
            .issue_session_challenge(
                SessionChallengeRequestV1 {
                    challenge_id: format!("challenge-{suffix}"),
                    subject_id: "owner-1".to_string(),
                    key_id: "owner-key-1".to_string(),
                    app_host_identity: "codex-app".to_string(),
                    session_context_digest: context_digest,
                    nonce: nonce.clone(),
                    issued_at: NOW - 5,
                    expires_at: NOW + 500,
                },
                keys,
                NOW,
            )
            .unwrap();
        let capability = test_capability(
            config,
            "runtime.session.handshake",
            challenge.challenge_digest,
            &nonce,
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        runtime
            .authenticate_session(
                &challenge.core.challenge_id,
                Ingress::Mcp,
                &capability,
                keys,
                NOW,
            )
            .unwrap()
    }

    fn verify_test_bootstrap(
        runtime: &AuthorityRuntime,
        config: &AuthorityRuntimeConfig,
        keys: &VerificationKeyRegistryV1,
        session: &AuthenticatedSessionV1,
        nonce: &str,
    ) -> AuthorityRuntimeStateV1 {
        let payload = runtime.status().unwrap().state.record_digest;
        let capability = test_capability(
            config,
            "brain.bootstrap",
            payload,
            nonce,
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        runtime
            .verify_bootstrap(
                &session.session_id,
                &session.session_context_digest,
                Ingress::Mcp,
                &capability,
                keys,
                NOW,
            )
            .unwrap()
    }

    fn catalog_entry(action: &str) -> ActionCatalogEntryV1 {
        m1nd10_action_catalog()
            .unwrap()
            .entries
            .into_iter()
            .find(|entry| entry.action.as_str() == action)
            .unwrap()
    }

    fn positive_request(
        action: &str,
        ingress: Ingress,
        payload_digest: String,
        session: &AuthenticatedSessionV1,
    ) -> AuthorityAuthorizationRequestV1 {
        AuthorityAuthorizationRequestV1 {
            session_id: Some(session.session_id.clone()),
            session_context_digest: Some(session.session_context_digest.clone()),
            transport_session_id: "transport-session-1".to_string(),
            ingress_context_digest: test_digest("ingress-context"),
            ingress,
            action: ActionId::new(action).unwrap(),
            payload_digest,
            requested_effects: catalog_entry(action).complete_effects,
            mission_id: None,
            mission_head_id: None,
            now_ms: NOW,
        }
    }

    fn policy_autonomy_config(root: &Path, action: &str) -> AuthorityRuntimeConfig {
        let mut config = test_config(root);
        let entry = catalog_entry(action);
        for ingress in &entry.ingresses {
            let tuple = ReachablePolicyTupleV1 {
                ingress: *ingress,
                action: entry.action.clone(),
                active_mode: ActiveMode::PolicyAutonomous,
                subject_id: "owner-1".to_string(),
                authority_variant: AuthorityVariant::Policy,
                applicable_grant_id: Some("grant-policy-1".to_string()),
                applicable_tier: Some(AutonomyTier::A3AutonomousLand),
                risk_class: entry.risk_class,
            };
            config.policy_registry.reachable_tuples.push(tuple.clone());
            config.policy_registry.rules.push(ActionPolicyRuleV1 {
                tuple,
                effects: entry.complete_effects.clone(),
            });
        }
        config.policy_registry.seal().unwrap();
        config.policy_registry_digest = config.policy_registry.policy_digest.clone();
        config
    }

    fn autonomy_projection(
        config: &AuthorityRuntimeConfig,
        state_digest: String,
    ) -> AutonomyManifestProjectionV1 {
        use crate::autonomy_manifest::{
            AUTHORITY_JOURNAL_ID, AUTONOMY_EPOCH_AUTHORITY_ID, AUTONOMY_MANIFEST_PROJECTION_SCHEMA,
            CONSTITUTION_AUTHORITY_ID, INTENT_CORE_STORE_AUTHORITY_ID,
            SENTINEL_OUTBOX_AUTHORITY_ID,
        };
        use m1nd_control::{AuthorityFact, AuthorityFreshness, AutonomyFact};

        let protected_root_digest = test_digest("g9-protected-root");
        let authority = |revision: &str, digest: String| AuthorityFact {
            revision: revision.to_string(),
            digest,
            observed_at: NOW,
            freshness: AuthorityFreshness::Fresh,
            status: AuthorityStatus::Available,
        };
        AutonomyManifestProjectionV1 {
            schema: AUTONOMY_MANIFEST_PROJECTION_SCHEMA.to_string(),
            organism_id: config.organism_id.clone(),
            repo_id: config.repo_id.clone(),
            brain_id: config.brain_id.clone(),
            observed_at: NOW,
            state_generation: 2,
            state_digest: state_digest.clone(),
            protected_root_digest,
            journal_sequence: 2,
            journal_record_digest: test_digest("g9-journal"),
            intent_store_root_digest: test_digest("g9-intents"),
            intent_count: 1,
            autonomy: AutonomyFact {
                supported_modes: BTreeSet::from([
                    "HUMAN_GATED".to_string(),
                    "POLICY_AUTONOMOUS".to_string(),
                ]),
                mechanically_proven_modes: BTreeSet::from(["POLICY_AUTONOMOUS".to_string()]),
                active_mode: "POLICY_AUTONOMOUS".to_string(),
                activation_receipt_id: "activation-policy-1".to_string(),
                constitution_digest: config.constitution_digest.clone(),
                constitution_epoch: config.constitution_epoch,
                safety_kernel_digest: config.safety_kernel_digest.clone(),
                autonomy_epoch: 1,
                grants_digest: config.grants_digest.clone(),
                quorum_policy_digest: test_digest("g9-quorum"),
                max_effective_tier_projection: "A3_AUTONOMOUS_LAND".to_string(),
                issuance_frozen: false,
                sentinel_safety_state: "HEALTHY".to_string(),
            },
            authorities: BTreeMap::from([
                (
                    AUTHORITY_JOURNAL_ID.to_string(),
                    authority("2", test_digest("g9-journal")),
                ),
                (
                    AUTONOMY_EPOCH_AUTHORITY_ID.to_string(),
                    authority("1", test_digest("g9-epoch")),
                ),
                (
                    CONSTITUTION_AUTHORITY_ID.to_string(),
                    authority(
                        &config.constitution_epoch.to_string(),
                        config.constitution_digest.clone(),
                    ),
                ),
                (
                    INTENT_CORE_STORE_AUTHORITY_ID.to_string(),
                    authority("1", test_digest("g9-intents")),
                ),
                (
                    SENTINEL_OUTBOX_AUTHORITY_ID.to_string(),
                    authority("0", state_digest),
                ),
            ]),
        }
    }

    struct FakeAutonomyOwner {
        projection: AutonomyManifestProjectionV1,
        post_admission_projection: Option<AutonomyManifestProjectionV1>,
        admission_count: AtomicUsize,
    }

    impl FakeAutonomyOwner {
        fn new(projection: AutonomyManifestProjectionV1) -> Self {
            Self {
                projection,
                post_admission_projection: None,
                admission_count: AtomicUsize::new(0),
            }
        }

        fn with_post_admission_projection(
            projection: AutonomyManifestProjectionV1,
            post_admission_projection: AutonomyManifestProjectionV1,
        ) -> Self {
            Self {
                projection,
                post_admission_projection: Some(post_admission_projection),
                admission_count: AtomicUsize::new(0),
            }
        }
    }

    impl crate::autonomy_manifest::AutonomyManifestReader for FakeAutonomyOwner {
        fn read_projection(
            &self,
            observed_at: u64,
        ) -> Result<
            AutonomyManifestProjectionV1,
            crate::autonomy_manifest::AutonomyManifestProjectionError,
        > {
            let mut projection = if self.admission_count.load(AtomicOrdering::SeqCst) > 0 {
                self.post_admission_projection
                    .as_ref()
                    .unwrap_or(&self.projection)
                    .clone()
            } else {
                self.projection.clone()
            };
            projection.observed_at = observed_at;
            for authority in projection.authorities.values_mut() {
                authority.observed_at = observed_at;
            }
            Ok(projection)
        }
    }

    impl AutonomyAdmissionOwner for FakeAutonomyOwner {
        fn assurance(&self) -> m1nd_control::autonomy_runtime::AutonomyRuntimeAssurance {
            m1nd_control::autonomy_runtime::AutonomyRuntimeAssurance::SoftwareTestOnlyNotProduction
        }

        fn admit(
            &self,
            evidence: &AutonomyAuthorityEvidenceV1,
            now_ms: u64,
        ) -> Result<
            AutonomyAdmissionOutcomeV1,
            crate::autonomy_manifest::AutonomyManifestProjectionError,
        > {
            let projection =
                crate::autonomy_manifest::AutonomyManifestReader::read_projection(self, now_ms)?;
            self.admission_count.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(AutonomyAdmissionOutcomeV1 {
                receipt: m1nd_control::autonomy_runtime::AutonomyAdmissionReceiptV1 {
                    schema: m1nd_control::autonomy_runtime::AUTONOMY_ADMISSION_RECEIPT_SCHEMA
                        .to_string(),
                    intent_digest: evidence.intent_digest.clone(),
                    decision_digest: evidence.decision.decision_digest().to_string(),
                    capability_digest: evidence.capability.capability_digest.clone(),
                    authority_variant: evidence.decision.authority_variant(),
                    committed_state_digest: projection.state_digest.clone(),
                    protected_root_digest: projection.protected_root_digest.clone(),
                    receipt_digest: test_digest("g9-admission-receipt"),
                },
                projection,
            })
        }
    }

    fn autonomy_evidence(
        config: &AuthorityRuntimeConfig,
        action: &str,
        payload_digest: &str,
        nonce: &str,
    ) -> AutonomyAuthorityEvidenceV1 {
        use m1nd_control::autonomy::{
            AuthorityDecisionBindingV1, AutonomyCapabilityCoreV1, AutonomyCapabilityV1,
            PolicyAuthorityDecisionCoreV1, PolicyAuthorityDecisionV1, AUTHORITY_DECISION_SCHEMA,
            AUTONOMY_CAPABILITY_SCHEMA,
        };

        let intent_digest = test_digest("g9-intent");
        let decision_digest = test_digest("g9-decision");
        let intent_core_ref =
            m1nd_control::autonomy::IntentCoreRefV1::for_sovereign_digest(intent_digest.clone());
        let binding = AuthorityDecisionBindingV1 {
            decision_id: "policy-decision-1".to_string(),
            intent_digest: intent_digest.clone(),
            intent_core_ref: intent_core_ref.clone(),
            intent_canonicalization_version: m1nd_control::CANONICALIZATION_VERSION.to_string(),
            required_authority_variant: AuthorityVariant::Policy,
            issuer_subject_id: "owner-1".to_string(),
            decision_subject_id: "policy-agent-1".to_string(),
            caller_subject_id: "owner-1".to_string(),
            audience: config.audience.clone(),
            proposer_subject_id: "proposer-agent-1".to_string(),
            executor_subject_id: Some("executor-agent-1".to_string()),
            promotion_target_subject_id: None,
            ratification_target_subject_id: None,
            delegation_grant_digest: None,
            action_policy_registry_digest: config.policy_registry_digest.clone(),
            classifier_decision_digest: test_digest("classifier"),
            constitution_digest: config.constitution_digest.clone(),
            constitution_epoch: config.constitution_epoch,
            autonomy_epoch: 1,
            active_mode: ActiveMode::PolicyAutonomous,
            grant_id: Some("grant-policy-1".to_string()),
            effective_tier: Some(AutonomyTier::A3AutonomousLand),
            action_class: "land".to_string(),
            semantic_action_id: action.to_string(),
            risk_class: catalog_entry(action).risk_class,
            risk_scope_digest: test_digest("risk-scope"),
            resource_environment_scope_digest: test_digest("resource-scope"),
            requested_budget: 1,
            sentinel_required: false,
            sentinel_verdict_digest: None,
            action_payload_digest: payload_digest.to_string(),
        };
        let decision =
            m1nd_control::autonomy::AuthorityDecisionV1::Policy(PolicyAuthorityDecisionV1 {
                schema: AUTHORITY_DECISION_SCHEMA.to_string(),
                core: PolicyAuthorityDecisionCoreV1 {
                    binding,
                    policy_digest: test_digest("policy"),
                    matched_clauses_digest: test_digest("clauses"),
                    risk_budget_scope_digest: test_digest("risk-budget"),
                    proof_receipts_digest: test_digest("proofs"),
                    sentinel_exemption_clause_digest: Some(test_digest("sentinel-exemption")),
                },
                decision_digest: decision_digest.clone(),
                owner_signature: OpaqueSignature::new("opaque-policy-signature"),
            });
        let capability = AutonomyCapabilityV1 {
            schema: AUTONOMY_CAPABILITY_SCHEMA.to_string(),
            core: AutonomyCapabilityCoreV1 {
                capability_id: format!("capability-{nonce}"),
                intent_digest: intent_digest.clone(),
                intent_core_ref,
                intent_canonicalization_version: m1nd_control::CANONICALIZATION_VERSION.to_string(),
                decision_digest,
                decision_policy_digest: test_digest("policy"),
                required_authority_variant: AuthorityVariant::Policy,
                action_policy_registry_digest: config.policy_registry_digest.clone(),
                classifier_decision_digest: test_digest("classifier"),
                constitution_digest: config.constitution_digest.clone(),
                constitution_epoch: config.constitution_epoch,
                autonomy_epoch: 1,
                organism_id: config.organism_id.clone(),
                repo_id: config.repo_id.clone(),
                issuer_subject_id: "owner-1".to_string(),
                decision_subject_id: "policy-agent-1".to_string(),
                caller_subject_id: "owner-1".to_string(),
                proposer_subject_id: "proposer-agent-1".to_string(),
                executor_subject_id: Some("executor-agent-1".to_string()),
                promotion_target_subject_id: None,
                ratification_target_subject_id: None,
                delegation_grant_digest: None,
                audience: config.audience.clone(),
                active_mode: ActiveMode::PolicyAutonomous,
                activation_receipt_id: Some("activation-policy-1".to_string()),
                grant_id: "grant-policy-1".to_string(),
                grant_digest: test_digest("grant-policy-1"),
                effective_tier: AutonomyTier::A3AutonomousLand,
                action_class: "land".to_string(),
                semantic_action_id: action.to_string(),
                risk_class: catalog_entry(action).risk_class,
                risk_scope_digest: test_digest("risk-scope"),
                sentinel_verdict_digest: None,
                brain_id: config.brain_id.clone(),
                mission_id: None,
                mission_head_id: None,
                block_id: None,
                candidate_digest: None,
                promotion_subject_id: None,
                resource_environment_scope_digest: test_digest("resource-scope"),
                requested_budget: 1,
                expected_store_epoch: 1,
                expected_store_version: 1,
                expected_boundary_version: 1,
                expected_contract_version: 1,
                idempotency_key: format!("idempotency-{nonce}"),
                payload_digest: payload_digest.to_string(),
                nonce: nonce.to_string(),
                issued_at: NOW - 10,
                expires_at: NOW + 1_000,
            },
            capability_digest: test_digest("g9-capability"),
            owner_signature: OpaqueSignature::new("opaque-autonomy-signature"),
        };
        AutonomyAuthorityEvidenceV1 {
            intent_digest,
            decision,
            capability,
            sentinel: None,
        }
    }

    #[derive(Default)]
    struct SoftwareTestSafetyVerifier;

    impl SafetyActuatorVerifier for SoftwareTestSafetyVerifier {
        fn verify(
            &mut self,
            attempt: &SafetyActuatorAttemptV1,
        ) -> Result<VerifiedSafetyActuatorV1, String> {
            Ok(VerifiedSafetyActuatorV1 {
                signed_body_digest: attempt
                    .signed_body_digest()
                    .map_err(|error| error.to_string())?,
                key_id: attempt.core.actuator_key_id.clone(),
                subject_id: attempt.core.actuator_subject_id.clone(),
                assurance: SafetyVerifierAssurance::SoftwareTestOnlyNotProven,
            })
        }
    }

    struct SoftwareTestServiceVerifier;

    impl ServiceIdentityVerifier for SoftwareTestServiceVerifier {
        fn verify(
            &mut self,
            assertion: &ServiceIdentityAssertionV1,
            _pinned: &PinnedServiceIdentityV1,
        ) -> Result<VerifiedServiceIdentityV1, String> {
            Ok(VerifiedServiceIdentityV1 {
                signed_body_digest: assertion
                    .signed_body_digest()
                    .map_err(|error| error.to_string())?,
                assurance: ServiceIdentityVerificationAssurance::SoftwareTestOnlyNotProven,
            })
        }
    }

    fn safety_attempt(
        config: &AuthorityRuntimeConfig,
        action: &str,
        payload_digest: String,
        nonce: &str,
    ) -> SafetyActuatorAttemptV1 {
        SafetyActuatorAttemptV1 {
            schema: SAFETY_ACTUATOR_ATTEMPT_SCHEMA.to_string(),
            core: SafetyActuatorAttemptCoreV1 {
                attempt_id: format!("attempt-{nonce}"),
                actuator_subject_id: "safety-actuator-1".to_string(),
                actuator_key_id: "safety-key-1".to_string(),
                actuator_identity_key_binary_policy_digest: config
                    .safety_actuator_identity_key_binary_policy_digest
                    .clone(),
                action: ActionId::new(action).unwrap(),
                payload_digest,
                negative_effects: catalog_entry(action).complete_effects,
                constitution_epoch: config.constitution_epoch,
                autonomy_epoch: 0,
                nonce: nonce.to_string(),
                issued_at: NOW - 10,
                expires_at: NOW + 1_000,
            },
            signature: OpaqueSignature::new("software-test-safety-signature"),
        }
    }

    #[test]
    fn bootstrap_is_human_gated_frozen_and_assurance_limits_are_explicit() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let backend = SoftwareTestProtectedEpochBackend::new();
        let runtime = AuthorityRuntime::bootstrap_software_test(config, backend, None).unwrap();
        let status = runtime.status().unwrap();
        assert_eq!(status.state.core.active_mode, ActiveMode::HumanGated);
        assert!(status.state.core.issuance_frozen);
        assert_eq!(status.state.core.safety_state, SafetyState::Frozen);
        assert_eq!(status.state.core.revision, 0);
        assert_eq!(status.state.core.protected_epoch, 1);
        assert_eq!(status.state.core.journal_sequence, 1);
        assert_eq!(status.state.core.replay_sequence, 0);
        assert_eq!(
            status.protected_epoch_assurance,
            ProtectedEpochAssurance::SoftwareTestOnlyNotProven
        );
        assert_eq!(
            status.positive_verification_assurance,
            AuthorityVerificationAssurance::SoftwareTestOnlyNotProven
        );
        assert!(status.semantic_catalog_entries >= 160);
        assert!(!status.transport_schema_parity_proven);
        assert!(!status.multi_artifact_atomicity_proven);
        assert!(status.automatic_crash_recovery_proven);
    }

    #[test]
    fn canonical_mode_matrix_rejects_agent_quorum_in_policy_autonomous() {
        assert!(mode_accepts(ActiveMode::HumanGated, AuthorityVariant::Human).is_ok());
        assert!(mode_accepts(ActiveMode::HumanGated, AuthorityVariant::Policy).is_err());
        assert!(mode_accepts(ActiveMode::PolicyAutonomous, AuthorityVariant::Human).is_ok());
        assert!(mode_accepts(ActiveMode::PolicyAutonomous, AuthorityVariant::Policy).is_ok());
        assert!(mode_accepts(ActiveMode::PolicyAutonomous, AuthorityVariant::AgentQuorum).is_err());
        assert!(mode_accepts(ActiveMode::FullAutonomy, AuthorityVariant::AgentQuorum).is_ok());
        assert!(mode_accepts(ActiveMode::FullAutonomy, AuthorityVariant::Policy).is_err());
    }

    #[test]
    fn protected_g9_synchronizes_before_autonomous_admission_and_binds_receipt() {
        let temp = TempDir::new().unwrap();
        let action = "system_blocks.ratify";
        let config = policy_autonomy_config(temp.path(), action);
        let keys = test_keys();
        let backend = SoftwareTestProtectedEpochBackend::new();
        let mut runtime =
            AuthorityRuntime::bootstrap_software_test(config.clone(), backend, None).unwrap();
        let session = authenticate_test_session(&runtime, &config, &keys, "g9-success");
        verify_test_bootstrap(&runtime, &config, &keys, &session, "g9-success-bootstrap");

        let owner = Arc::new(FakeAutonomyOwner::new(autonomy_projection(
            &config,
            test_digest("g9-state-success"),
        )));
        runtime
            .install_autonomy_admission_owner(owner.clone())
            .unwrap();

        let payload = test_digest("g9-autonomous-payload");
        let nonce = "g9-autonomous-success";
        let capability = test_capability(
            &config,
            action,
            payload.clone(),
            nonce,
            AuthorityVariant::Policy,
            ActiveMode::PolicyAutonomous,
            None,
            None,
        );
        let evidence = autonomy_evidence(&config, action, &payload, nonce);
        let metadata = PositiveSovereignAuthorityMetadataV1 {
            role: Role::Author,
            capability_kind: CapabilityKind::Autonomy,
            authority_decision_digest: evidence.decision.decision_digest().to_string(),
            applicable_grant_id: Some("grant-policy-1".to_string()),
            applicable_tier: Some(AutonomyTier::A3AutonomousLand),
        };
        let receipt = runtime
            .authorize_mutation(
                positive_request(action, Ingress::Mcp, payload, &session),
                AuthorityInputV1::PositiveSovereign {
                    capability: &capability,
                    keys: &keys,
                    metadata: &metadata,
                    autonomy_evidence: Some(&evidence),
                },
            )
            .unwrap();

        assert_eq!(owner.admission_count.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(
            runtime.status().unwrap().state.core.active_mode,
            ActiveMode::PolicyAutonomous
        );
        assert_eq!(
            receipt.core.autonomy_admission_receipt_digest.as_deref(),
            Some(test_digest("g9-admission-receipt").as_str())
        );
        assert_eq!(
            receipt.core.autonomy_committed_state_digest.as_deref(),
            Some(test_digest("g9-state-success").as_str())
        );
        assert!(matches!(
            receipt.core.authority,
            AuthorizationAuthorityV1::Autonomous {
                variant: AuthorityVariant::Policy,
                ..
            }
        ));
    }

    #[test]
    fn post_admission_g9_drift_burns_liveness_and_freezes_before_receipt_escapes() {
        let temp = TempDir::new().unwrap();
        let action = "system_blocks.ratify";
        let config = policy_autonomy_config(temp.path(), action);
        let keys = test_keys();
        let backend = SoftwareTestProtectedEpochBackend::new();
        let mut runtime =
            AuthorityRuntime::bootstrap_software_test(config.clone(), backend, None).unwrap();
        let session = authenticate_test_session(&runtime, &config, &keys, "g9-post-drift");
        verify_test_bootstrap(
            &runtime,
            &config,
            &keys,
            &session,
            "g9-post-drift-bootstrap",
        );

        let admission_projection =
            autonomy_projection(&config, test_digest("g9-state-before-post-drift"));
        let mut post_projection = admission_projection.clone();
        post_projection.state_digest = test_digest("g9-state-after-post-drift");
        post_projection.protected_root_digest = test_digest("g9-root-after-post-drift");
        let owner = Arc::new(FakeAutonomyOwner::with_post_admission_projection(
            admission_projection,
            post_projection,
        ));
        runtime
            .install_autonomy_admission_owner(owner.clone())
            .unwrap();

        let payload = test_digest("g9-post-drift-payload");
        let nonce = "g9-post-drift";
        let capability = test_capability(
            &config,
            action,
            payload.clone(),
            nonce,
            AuthorityVariant::Policy,
            ActiveMode::PolicyAutonomous,
            None,
            None,
        );
        let evidence = autonomy_evidence(&config, action, &payload, nonce);
        let metadata = PositiveSovereignAuthorityMetadataV1 {
            role: Role::Author,
            capability_kind: CapabilityKind::Autonomy,
            authority_decision_digest: evidence.decision.decision_digest().to_string(),
            applicable_grant_id: Some("grant-policy-1".to_string()),
            applicable_tier: Some(AutonomyTier::A3AutonomousLand),
        };
        let error = runtime
            .authorize_mutation(
                positive_request(action, Ingress::Mcp, payload, &session),
                AuthorityInputV1::PositiveSovereign {
                    capability: &capability,
                    keys: &keys,
                    metadata: &metadata,
                    autonomy_evidence: Some(&evidence),
                },
            )
            .unwrap_err();
        assert!(matches!(
            error,
            AuthorityRuntimeError::AutonomyMirrorMismatch {
                field: "post_authorization_state_digest"
            }
        ));
        let status = runtime.status().unwrap();
        assert!(status.state.core.issuance_frozen);
        assert_eq!(status.state.core.safety_state, SafetyState::Frozen);
        assert_eq!(owner.admission_count.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn autonomous_generic_bypass_and_missing_evidence_never_reach_g9_consume() {
        let temp = TempDir::new().unwrap();
        let action = "system_blocks.ratify";
        let config = policy_autonomy_config(temp.path(), action);
        let keys = test_keys();
        let backend = SoftwareTestProtectedEpochBackend::new();
        let mut runtime =
            AuthorityRuntime::bootstrap_software_test(config.clone(), backend, None).unwrap();
        let session = authenticate_test_session(&runtime, &config, &keys, "g9-bypass");
        verify_test_bootstrap(&runtime, &config, &keys, &session, "g9-bypass-bootstrap");
        let owner = Arc::new(FakeAutonomyOwner::new(autonomy_projection(
            &config,
            test_digest("g9-state-bypass"),
        )));
        runtime
            .install_autonomy_admission_owner(owner.clone())
            .unwrap();

        let payload = test_digest("g9-bypass-payload");
        let capability = test_capability(
            &config,
            action,
            payload.clone(),
            "g9-bypass",
            AuthorityVariant::Policy,
            ActiveMode::PolicyAutonomous,
            None,
            None,
        );
        let generic_error = runtime
            .authorize_mutation(
                positive_request(action, Ingress::Mcp, payload.clone(), &session),
                AuthorityInputV1::Positive {
                    capability: &capability,
                    keys: &keys,
                },
            )
            .unwrap_err();
        assert!(matches!(
            generic_error,
            AuthorityRuntimeError::AutonomyAdmissionUnavailable
        ));
        assert_eq!(owner.admission_count.load(AtomicOrdering::SeqCst), 0);

        let evidence = autonomy_evidence(&config, action, &payload, "g9-bypass");
        let metadata = PositiveSovereignAuthorityMetadataV1 {
            role: Role::Author,
            capability_kind: CapabilityKind::Autonomy,
            authority_decision_digest: evidence.decision.decision_digest().to_string(),
            applicable_grant_id: Some("grant-policy-1".to_string()),
            applicable_tier: Some(AutonomyTier::A3AutonomousLand),
        };
        let missing_error = runtime
            .authorize_mutation(
                positive_request(action, Ingress::Mcp, payload, &session),
                AuthorityInputV1::PositiveSovereign {
                    capability: &capability,
                    keys: &keys,
                    metadata: &metadata,
                    autonomy_evidence: None,
                },
            )
            .unwrap_err();
        assert!(matches!(
            missing_error,
            AuthorityRuntimeError::AutonomyAdmissionUnavailable
        ));
        assert_eq!(owner.admission_count.load(AtomicOrdering::SeqCst), 0);

        let mut foreign_repo_evidence = evidence.clone();
        foreign_repo_evidence.capability.core.repo_id = "foreign-repo".to_string();
        let binding_error = runtime
            .authorize_mutation(
                positive_request(
                    action,
                    Ingress::Mcp,
                    test_digest("g9-bypass-payload"),
                    &session,
                ),
                AuthorityInputV1::PositiveSovereign {
                    capability: &capability,
                    keys: &keys,
                    metadata: &metadata,
                    autonomy_evidence: Some(&foreign_repo_evidence),
                },
            )
            .unwrap_err();
        assert!(matches!(
            binding_error,
            AuthorityRuntimeError::AutonomyAdmission { .. }
        ));
        assert_eq!(owner.admission_count.load(AtomicOrdering::SeqCst), 0);

        let mut wrong_action_evidence = evidence.clone();
        wrong_action_evidence.capability.core.semantic_action_id =
            "mission.service.land".to_string();
        let action_error = runtime
            .authorize_mutation(
                positive_request(
                    action,
                    Ingress::Mcp,
                    test_digest("g9-bypass-payload"),
                    &session,
                ),
                AuthorityInputV1::PositiveSovereign {
                    capability: &capability,
                    keys: &keys,
                    metadata: &metadata,
                    autonomy_evidence: Some(&wrong_action_evidence),
                },
            )
            .unwrap_err();
        assert!(matches!(
            action_error,
            AuthorityRuntimeError::AutonomyAdmission { .. }
        ));
        assert_eq!(owner.admission_count.load(AtomicOrdering::SeqCst), 0);

        let mut wrong_time_evidence = evidence.clone();
        wrong_time_evidence.capability.core.expires_at += 1;
        let time_error = runtime
            .authorize_mutation(
                positive_request(
                    action,
                    Ingress::Mcp,
                    test_digest("g9-bypass-payload"),
                    &session,
                ),
                AuthorityInputV1::PositiveSovereign {
                    capability: &capability,
                    keys: &keys,
                    metadata: &metadata,
                    autonomy_evidence: Some(&wrong_time_evidence),
                },
            )
            .unwrap_err();
        assert!(matches!(
            time_error,
            AuthorityRuntimeError::AutonomyAdmission { .. }
        ));
        assert_eq!(owner.admission_count.load(AtomicOrdering::SeqCst), 0);

        let human_capability = test_capability(
            &config,
            action,
            test_digest("g9-bypass-payload"),
            "human-forbids-evidence",
            AuthorityVariant::Human,
            ActiveMode::PolicyAutonomous,
            None,
            None,
        );
        let human_metadata = PositiveSovereignAuthorityMetadataV1 {
            role: Role::Author,
            capability_kind: CapabilityKind::Human,
            authority_decision_digest: test_digest("human-decision"),
            applicable_grant_id: None,
            applicable_tier: None,
        };
        let human_error = runtime
            .authorize_mutation(
                positive_request(
                    action,
                    Ingress::Mcp,
                    test_digest("g9-bypass-payload"),
                    &session,
                ),
                AuthorityInputV1::PositiveSovereign {
                    capability: &human_capability,
                    keys: &keys,
                    metadata: &human_metadata,
                    autonomy_evidence: Some(&evidence),
                },
            )
            .unwrap_err();
        assert!(matches!(
            human_error,
            AuthorityRuntimeError::BindingMismatch {
                field: "human_authority_forbids_autonomy_evidence"
            }
        ));
        assert_eq!(owner.admission_count.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn foreign_g9_scope_freezes_human_positive_authority_globally() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let keys = test_keys();
        let backend = SoftwareTestProtectedEpochBackend::new();
        let mut runtime =
            AuthorityRuntime::bootstrap_software_test(config.clone(), backend, None).unwrap();
        let session = authenticate_test_session(&runtime, &config, &keys, "g9-foreign");
        verify_test_bootstrap(&runtime, &config, &keys, &session, "g9-foreign-bootstrap");
        let mut projection = autonomy_projection(&config, test_digest("g9-state-foreign"));
        projection.repo_id = "foreign-repo".to_string();
        let owner = Arc::new(FakeAutonomyOwner::new(projection));
        runtime
            .install_autonomy_admission_owner(owner.clone())
            .unwrap();

        let payload = test_digest("human-after-foreign-g9");
        let capability = test_capability(
            &config,
            "system_blocks.ratify",
            payload.clone(),
            "human-after-foreign-g9",
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        let error = runtime
            .authorize_mutation(
                positive_request("system_blocks.ratify", Ingress::Mcp, payload, &session),
                AuthorityInputV1::Positive {
                    capability: &capability,
                    keys: &keys,
                },
            )
            .unwrap_err();
        assert!(matches!(
            error,
            AuthorityRuntimeError::AutonomyMirrorMismatch { field: "repo_id" }
        ));
        let status = runtime.status().unwrap();
        assert!(status.state.core.issuance_frozen);
        assert_eq!(status.state.core.safety_state, SafetyState::Frozen);
        assert_eq!(owner.admission_count.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn positive_authority_stays_frozen_until_authenticated_bootstrap_verification() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let keys = test_keys();
        let backend = SoftwareTestProtectedEpochBackend::new();
        let runtime =
            AuthorityRuntime::bootstrap_software_test(config.clone(), backend, None).unwrap();
        let session = authenticate_test_session(&runtime, &config, &keys, "freeze");
        assert!(runtime.status().unwrap().state.core.issuance_frozen);

        let payload = test_digest("presence-mutation");
        let capability = test_capability(
            &config,
            "system_blocks.ratify",
            payload.clone(),
            "mutation-before-bootstrap",
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        let error = runtime
            .authorize_mutation(
                positive_request(
                    "system_blocks.ratify",
                    Ingress::Mcp,
                    payload.clone(),
                    &session,
                ),
                AuthorityInputV1::Positive {
                    capability: &capability,
                    keys: &keys,
                },
            )
            .unwrap_err();
        assert!(matches!(error, AuthorityRuntimeError::IssuanceFrozen));

        let verified = verify_test_bootstrap(&runtime, &config, &keys, &session, "bootstrap-human");
        assert!(!verified.core.issuance_frozen);
        assert_eq!(verified.core.safety_state, SafetyState::Healthy);
        let receipt = runtime
            .authorize_mutation(
                positive_request("system_blocks.ratify", Ingress::Mcp, payload, &session),
                AuthorityInputV1::Positive {
                    capability: &capability,
                    keys: &keys,
                },
            )
            .unwrap();
        assert!(matches!(
            receipt.core.authority,
            AuthorizationAuthorityV1::Positive {
                assurance: AuthorityVerificationAssurance::SoftwareTestOnlyNotProven,
                ..
            }
        ));
        let status = runtime.status().unwrap();
        assert_eq!(status.state.core.replay_sequence, 3);
        assert_eq!(status.state.core.journal_sequence, 4);
        assert_eq!(status.state.core.protected_epoch, 4);
    }

    #[test]
    fn session_registry_binds_context_nonce_and_current_key_lifecycle() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let keys = test_keys();
        let runtime = AuthorityRuntime::bootstrap_software_test(
            config.clone(),
            SoftwareTestProtectedEpochBackend::new(),
            None,
        )
        .unwrap();
        let session = authenticate_test_session(&runtime, &config, &keys, "lifecycle");
        let payload = runtime.status().unwrap().state.record_digest;
        let capability = test_capability(
            &config,
            "brain.bootstrap",
            payload,
            "bootstrap-lifecycle",
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        let context_error = runtime
            .verify_bootstrap(
                &session.session_id,
                &test_digest("wrong-context"),
                Ingress::Mcp,
                &capability,
                &keys,
                NOW,
            )
            .unwrap_err();
        assert!(matches!(
            context_error,
            AuthorityRuntimeError::SessionContextMismatch
        ));

        let mut revoked = keys.clone();
        let key = revoked.keys.get_mut("owner-key-1").unwrap();
        key.status = IdentityStatus::Revoked;
        key.revoked_at = Some(NOW);
        let lifecycle_error = runtime
            .verify_bootstrap(
                &session.session_id,
                &session.session_context_digest,
                Ingress::Mcp,
                &capability,
                &revoked,
                NOW,
            )
            .unwrap_err();
        assert!(matches!(
            lifecycle_error,
            AuthorityRuntimeError::SessionKeyInactive { .. }
        ));

        let consumed_error = runtime
            .authenticate_session(
                "challenge-lifecycle",
                Ingress::Mcp,
                &test_capability(
                    &config,
                    "runtime.session.handshake",
                    test_digest("unused"),
                    "session-nonce-lifecycle",
                    AuthorityVariant::Human,
                    ActiveMode::HumanGated,
                    None,
                    None,
                ),
                &keys,
                NOW,
            )
            .unwrap_err();
        assert!(matches!(
            consumed_error,
            AuthorityRuntimeError::ChallengeConsumed { .. }
        ));
    }

    #[test]
    fn restart_requires_reauthentication_and_persisted_nonce_replay_is_refused() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let keys = test_keys();
        let protected = SoftwareTestProtectedEpochBackend::new();
        let runtime =
            AuthorityRuntime::bootstrap_software_test(config.clone(), protected.clone(), None)
                .unwrap();
        let challenge_request = SessionChallengeRequestV1 {
            challenge_id: "challenge-before-restart".to_string(),
            subject_id: "owner-1".to_string(),
            key_id: "owner-key-1".to_string(),
            app_host_identity: "h4nd-fixture".to_string(),
            session_context_digest: test_digest("restart-wire-context"),
            nonce: "restart-persisted-nonce".to_string(),
            issued_at: NOW - 5,
            expires_at: NOW + 500,
        };
        let challenge = runtime
            .issue_session_challenge(challenge_request.clone(), &keys, NOW)
            .unwrap();
        let capability = test_capability(
            &config,
            "runtime.session.handshake",
            challenge.challenge_digest,
            &challenge.core.nonce,
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        let old_session = runtime
            .authenticate_session(
                &challenge.core.challenge_id,
                Ingress::Mcp,
                &capability,
                &keys,
                NOW,
            )
            .unwrap();
        drop(runtime);

        let reopened =
            AuthorityRuntime::open_software_test(config.clone(), protected, None).unwrap();
        assert!(reopened
            .authenticated_session(&old_session.session_id, NOW + 1)
            .unwrap()
            .is_none());
        assert!(matches!(
            reopened.verify_bootstrap(
                &old_session.session_id,
                &old_session.session_context_digest,
                Ingress::Mcp,
                &test_capability(
                    &config,
                    "brain.bootstrap",
                    reopened.status().unwrap().state.record_digest,
                    "old-session-bootstrap",
                    AuthorityVariant::Human,
                    ActiveMode::HumanGated,
                    None,
                    None,
                ),
                &keys,
                NOW + 1,
            ),
            Err(AuthorityRuntimeError::SessionNotFound { .. })
        ));

        // The in-memory challenge registry is intentionally empty after a
        // restart, but the durable replay ledger still rejects the exact
        // already-authenticated signed capability/nonce.
        reopened
            .issue_session_challenge(challenge_request, &keys, NOW + 1)
            .unwrap();
        assert!(matches!(
            reopened.authenticate_session(
                "challenge-before-restart",
                Ingress::Mcp,
                &capability,
                &keys,
                NOW + 1,
            ),
            Err(AuthorityRuntimeError::Replay(_))
        ));

        let new_session = authenticate_test_session(&reopened, &config, &keys, "after-restart");
        assert_ne!(new_session.session_id, old_session.session_id);
    }

    #[test]
    fn challenge_key_provenance_and_rotated_session_key_fail_closed() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let keys = test_keys();
        let runtime = AuthorityRuntime::bootstrap_software_test(
            config.clone(),
            SoftwareTestProtectedEpochBackend::new(),
            None,
        )
        .unwrap();
        assert!(matches!(
            runtime.issue_session_challenge(
                SessionChallengeRequestV1 {
                    challenge_id: "wrong-key-subject".to_string(),
                    subject_id: "not-owner-1".to_string(),
                    key_id: "owner-key-1".to_string(),
                    app_host_identity: "h4nd-fixture".to_string(),
                    session_context_digest: test_digest("wrong-key-subject-context"),
                    nonce: "wrong-key-subject-nonce".to_string(),
                    issued_at: NOW - 1,
                    expires_at: NOW + 100,
                },
                &keys,
                NOW,
            ),
            Err(AuthorityRuntimeError::Crypto(
                m1nd_control::AuthorityCryptoError::KeySubjectMismatch { .. }
            ))
        ));

        let session = authenticate_test_session(&runtime, &config, &keys, "rotation");
        let mut rotated = keys.clone();
        rotated.registry_epoch += 1;
        let mut replacement = rotated.keys["owner-key-1"].clone();
        replacement.key_id = "owner-key-2".to_string();
        replacement.created_at = 50;
        replacement.activated_at = 51;
        let old = rotated.keys.get_mut("owner-key-1").unwrap();
        old.status = IdentityStatus::Rotated;
        old.rotated_at = Some(NOW);
        old.replacement_key_id = Some(replacement.key_id.clone());
        rotated.keys.insert(replacement.key_id.clone(), replacement);
        assert!(matches!(
            runtime.verify_bootstrap(
                &session.session_id,
                &session.session_context_digest,
                Ingress::Mcp,
                &test_capability(
                    &config,
                    "brain.bootstrap",
                    runtime.status().unwrap().state.record_digest,
                    "rotated-session-bootstrap",
                    AuthorityVariant::Human,
                    ActiveMode::HumanGated,
                    None,
                    None,
                ),
                &rotated,
                NOW,
            ),
            Err(AuthorityRuntimeError::SessionKeyInactive { .. })
        ));
    }

    #[test]
    fn semantic_gate_rejects_unknown_unreachable_effect_drift_and_uncovered_service_floor() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let keys = test_keys();
        let runtime = AuthorityRuntime::bootstrap_software_test(
            config.clone(),
            SoftwareTestProtectedEpochBackend::new(),
            None,
        )
        .unwrap();
        let capability = test_capability(
            &config,
            "unknown.action",
            test_digest("payload"),
            "unknown",
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        let unknown = runtime
            .authorize_mutation(
                AuthorityAuthorizationRequestV1 {
                    session_id: None,
                    session_context_digest: None,
                    transport_session_id: "transport-session-1".to_string(),
                    ingress_context_digest: test_digest("ingress-context"),
                    ingress: Ingress::Mcp,
                    action: ActionId::new("unknown.action").unwrap(),
                    payload_digest: test_digest("payload"),
                    requested_effects: BTreeSet::from([Effect::Read]),
                    mission_id: None,
                    mission_head_id: None,
                    now_ms: NOW,
                },
                AuthorityInputV1::Positive {
                    capability: &capability,
                    keys: &keys,
                },
            )
            .unwrap_err();
        assert!(matches!(
            unknown,
            AuthorityRuntimeError::UnknownAction { .. }
        ));

        let entry = catalog_entry("runtime.presence.track_agent");
        let unreachable = runtime
            .authorize_mutation(
                AuthorityAuthorizationRequestV1 {
                    session_id: None,
                    session_context_digest: None,
                    transport_session_id: "transport-session-1".to_string(),
                    ingress_context_digest: test_digest("ingress-context"),
                    ingress: Ingress::Cli,
                    action: entry.action.clone(),
                    payload_digest: test_digest("payload"),
                    requested_effects: entry.complete_effects.clone(),
                    mission_id: None,
                    mission_head_id: None,
                    now_ms: NOW,
                },
                AuthorityInputV1::Positive {
                    capability: &capability,
                    keys: &keys,
                },
            )
            .unwrap_err();
        assert!(matches!(
            unreachable,
            AuthorityRuntimeError::UnreachableIngress { .. }
        ));

        let effect_drift = runtime
            .authorize_mutation(
                AuthorityAuthorizationRequestV1 {
                    session_id: None,
                    session_context_digest: None,
                    transport_session_id: "transport-session-1".to_string(),
                    ingress_context_digest: test_digest("ingress-context"),
                    ingress: Ingress::Mcp,
                    action: entry.action,
                    payload_digest: test_digest("payload"),
                    requested_effects: BTreeSet::from([Effect::Read]),
                    mission_id: None,
                    mission_head_id: None,
                    now_ms: NOW,
                },
                AuthorityInputV1::Positive {
                    capability: &capability,
                    keys: &keys,
                },
            )
            .unwrap_err();
        assert!(matches!(
            effect_drift,
            AuthorityRuntimeError::BindingMismatch {
                field: "complete_effects"
            }
        ));

        let service = catalog_entry("runtime.root.self_heal");
        let uncovered = runtime
            .authorize_mutation(
                AuthorityAuthorizationRequestV1 {
                    session_id: None,
                    session_context_digest: None,
                    transport_session_id: "transport-session-1".to_string(),
                    ingress_context_digest: test_digest("ingress-context"),
                    ingress: Ingress::Cli,
                    action: service.action,
                    payload_digest: test_digest("payload"),
                    requested_effects: service.complete_effects,
                    mission_id: None,
                    mission_head_id: None,
                    now_ms: NOW,
                },
                AuthorityInputV1::Positive {
                    capability: &capability,
                    keys: &keys,
                },
            )
            .unwrap_err();
        assert!(matches!(
            uncovered,
            AuthorityRuntimeError::UncoveredAuthorityFloor {
                floor: AuthorityFloor::ServiceIdentity,
                ..
            }
        ));
    }

    #[test]
    fn ordinary_read_and_pinned_service_identity_issue_fully_bound_receipts() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let keys = test_keys();
        let runtime = AuthorityRuntime::bootstrap_software_test(
            config.clone(),
            SoftwareTestProtectedEpochBackend::new(),
            None,
        )
        .unwrap();
        let session = authenticate_test_session(&runtime, &config, &keys, "ordinary-session");
        let read_object = test_digest("land-intent-read-object");
        let read_receipt = runtime
            .authorize_mutation(
                positive_request(
                    "mission.service.land_intent",
                    Ingress::Mcp,
                    read_object.clone(),
                    &session,
                ),
                AuthorityInputV1::OrdinarySession {
                    keys: &keys,
                    role: Role::Author,
                },
            )
            .unwrap();
        assert_eq!(read_receipt.core.subject_id, "owner-1");
        assert_eq!(read_receipt.core.role, Role::Author);
        assert_eq!(read_receipt.core.capability_kind, None);
        assert_eq!(read_receipt.core.verified_object_digest, read_object);
        assert_eq!(
            read_receipt.core.transport_session_id,
            "transport-session-1"
        );
        assert!(matches!(
            read_receipt.core.authority,
            AuthorizationAuthorityV1::OrdinarySession { .. }
        ));

        runtime
            .install_service_identity_verifier(Box::new(SoftwareTestServiceVerifier))
            .unwrap();
        let action = ActionId::new("mission.service.execution_started").unwrap();
        let service_object = test_digest("service-object");
        let ingress_context_digest = test_digest("service-ingress");
        let assertion = ServiceIdentityAssertionV1 {
            schema: SERVICE_IDENTITY_ASSERTION_SCHEMA.to_string(),
            core: ServiceIdentityAssertionCoreV1 {
                service_id: "runnerd-1".to_string(),
                subject_id: "runner-service-1".to_string(),
                key_id: "runner-key-1".to_string(),
                role: Role::Runner,
                organism_id: config.organism_id.clone(),
                brain_id: config.brain_id.clone(),
                audience: config.audience.clone(),
                identity_key_binary_policy_digest: test_digest("runner-pin"),
                action: action.clone(),
                object_digest: service_object.clone(),
                mission_id: Some("mission-1".to_string()),
                mission_head_id: Some("head-1".to_string()),
                transport_session_id: "runner-transport-1".to_string(),
                ingress_context_digest: ingress_context_digest.clone(),
                nonce: "runner-nonce-1".to_string(),
                issued_at: NOW - 1,
                expires_at: NOW + 1_000,
            },
            signature: OpaqueSignature::new("software-test-service-signature"),
        };
        let service_receipt = runtime
            .authorize_mutation(
                AuthorityAuthorizationRequestV1 {
                    session_id: None,
                    session_context_digest: None,
                    transport_session_id: "runner-transport-1".to_string(),
                    ingress_context_digest,
                    ingress: Ingress::Rest,
                    action,
                    payload_digest: service_object,
                    requested_effects: catalog_entry("mission.service.execution_started")
                        .complete_effects,
                    mission_id: Some("mission-1".to_string()),
                    mission_head_id: Some("head-1".to_string()),
                    now_ms: NOW,
                },
                AuthorityInputV1::ServiceIdentity {
                    assertion: &assertion,
                },
            )
            .unwrap();
        assert_eq!(service_receipt.core.subject_id, "runner-service-1");
        assert_eq!(service_receipt.core.role, Role::Runner);
        assert_eq!(
            service_receipt.core.mission_id.as_deref(),
            Some("mission-1")
        );
        assert!(matches!(
            service_receipt.core.authority,
            AuthorizationAuthorityV1::ServiceIdentity {
                assurance: ServiceIdentityVerificationAssurance::SoftwareTestOnlyNotProven,
                ..
            }
        ));
    }

    #[test]
    fn owner_authority_transport_issues_exact_one_shot_land_intent_lease() {
        use crate::authority_transport::{
            owner_authority_components, AuthorityAuthorizeInputV1, AuthorityAuthorizeRequestV1,
            AUTHORITY_AUTHORIZE_REQUEST_SCHEMA,
        };
        use crate::mission_service_transport::{
            ExternalMissionServiceRequestV1, MissionServiceAuthorityProvider,
            MissionServiceIngressV1, MissionServiceTransportContextV1,
            MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA,
        };
        use crate::owner_authorization_broker::{
            AuthorizationLeaseStateV1, OwnerAuthorityLinearizationV1,
            OwnerAuthorizationBrokerConfigV1, OwnerAuthorizationBrokerV1,
        };

        let temp = TempDir::new().unwrap();
        let runtime_root = temp.path().join("authority-runtime");
        let config = test_config(&runtime_root);
        let keys = test_keys();
        let runtime = Arc::new(
            AuthorityRuntime::bootstrap_software_test(
                config.clone(),
                SoftwareTestProtectedEpochBackend::new(),
                None,
            )
            .unwrap(),
        );
        let session = authenticate_test_session(&runtime, &config, &keys, "transport-lease");
        let broker_config = OwnerAuthorizationBrokerConfigV1 {
            root: temp.path().join("authorization-broker"),
            reservation_ttl_ms: 1_000,
            minimum_terminal_retention_ms: 1_000,
        };
        let linearization = OwnerAuthorityLinearizationV1::default();
        let (authority_service, mission_provider) = owner_authority_components(
            crate::authority_transport::OwnerAuthorityComponentInputsV1 {
            runtime: Arc::clone(&runtime),
            verification_keys: Arc::new(keys),
            session_roles: Arc::new(BTreeMap::from([("owner-1".to_string(), Role::Author)])),
            max_future_clock_skew_ms: 10,
            receipt_crypto: Arc::new(
                crate::authority_wal::SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(
                    b"authority-receipt-test-only",
                ),
            ),
            broker_config: broker_config.clone(),
            linearization: linearization.clone(),
            protected_journal_head: crate::protected_journal_head::SoftwareTestProtectedJournalHeadBackendV1::new()
                .shared(),
            },
        );
        let ingress_context_digest = session.session_context_digest.clone();
        let transport_context = MissionServiceTransportContextV1 {
            ingress: MissionServiceIngressV1::McpStreamableHttp,
            transport_session_id: Some("wire-session-1".to_string()),
            ingress_context_digest: Some(ingress_context_digest.clone()),
            authority_lease_id: None,
            caller_root: Some("/workspace".to_string()),
            route_selector: Some(config.brain_id.clone()),
            actor_brain_id: Some(config.brain_id.clone()),
        };
        let external_request = ExternalMissionServiceRequestV1::LandIntent {
            schema: MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA.to_string(),
            request_id: "land-intent-correlation-1".to_string(),
            mission_id: "mission-1".to_string(),
            expected_head_id: "head-1".to_string(),
            candidate_id: "candidate-1".to_string(),
            expected_candidate_digest: test_digest("candidate-1"),
            expected_store_version: 7,
            idempotency_key: "land-intent-idempotency-1".to_string(), // gitleaks:allow
        };
        let object_digest = external_request.authority_object_digest().unwrap();
        let authorize_request = AuthorityAuthorizeRequestV1 {
            schema: AUTHORITY_AUTHORIZE_REQUEST_SCHEMA.to_string(),
            request_id: "authority-request-1".to_string(),
            authority_session_id: Some(session.session_id.clone()),
            authority_session_context_digest: Some(session.session_context_digest.clone()),
            target_action: external_request.semantic_action_id().to_string(),
            payload_digest: object_digest.clone(),
            requested_effects: catalog_entry(external_request.semantic_action_id())
                .complete_effects,
            mission_id: Some("mission-1".to_string()),
            mission_head_id: Some("head-1".to_string()),
            input: AuthorityAuthorizeInputV1::OrdinarySession { role: Role::Author },
        };
        let mut asserted_wrong_role = authorize_request.clone();
        asserted_wrong_role.request_id = "authority-request-wrong-role".to_string();
        asserted_wrong_role.input = AuthorityAuthorizeInputV1::OrdinarySession {
            role: Role::Reviewer,
        };
        assert_eq!(
            authority_service
                .authorize(&transport_context, asserted_wrong_role, NOW)
                .unwrap_err()
                .code(),
            "authority_session_role_mismatch"
        );
        let response = authority_service
            .authorize(&transport_context, authorize_request, NOW)
            .unwrap();
        assert_eq!(
            response.authorization_receipt.core.transport_session_id,
            "wire-session-1"
        );
        assert_eq!(
            response.authorization_receipt.core.ingress_context_digest,
            ingress_context_digest
        );
        assert_eq!(
            response.authorization_receipt.core.verified_object_digest,
            object_digest
        );
        {
            let broker =
                OwnerAuthorizationBrokerV1::open(broker_config.clone(), linearization.clone())
                    .unwrap();
            assert_eq!(
                broker
                    .lease(&response.authorization_lease_id)
                    .unwrap()
                    .state,
                AuthorizationLeaseStateV1::Unused
            );
        }

        let reserved_context = MissionServiceTransportContextV1 {
            authority_lease_id: Some(response.authorization_lease_id.clone()),
            ..transport_context.clone()
        };
        let authority = mission_provider
            .authenticated_authority(
                &reserved_context,
                &external_request,
                &object_digest,
                NOW + 1,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            authority.authorization_lease_id,
            response.authorization_lease_id
        );
        assert_eq!(authority.verified_object_digest, object_digest);
        let replay = mission_provider
            .authenticated_authority(
                &reserved_context,
                &external_request,
                &authority.verified_object_digest,
                NOW + 2,
            )
            .unwrap_err();
        assert_eq!(replay.code(), "authorization_lease_not_unused");
    }

    #[test]
    fn production_session_ceremony_and_positive_land_transport_use_real_fixture_crypto() {
        use crate::authority_transport::{
            owner_authority_components, AuthorityAuthorizeInputV1, AuthorityAuthorizeRequestV1,
            AuthoritySessionAuthenticateRequestV1, AuthoritySessionChallengeRequestV1,
            AuthoritySessionVerificationAssuranceV1, AUTHORITY_AUTHORIZE_REQUEST_SCHEMA,
            AUTHORITY_SESSION_AUTHENTICATE_REQUEST_SCHEMA,
            AUTHORITY_SESSION_CHALLENGE_REQUEST_SCHEMA,
        };
        use crate::mission_service::{LandRequestV1, LAND_REQUEST_SCHEMA};
        use crate::mission_service_transport::{
            ExternalMissionServiceRequestV1, MissionServiceAuthorityProvider,
            MissionServiceIngressV1, MissionServiceTransportContextV1,
            MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA,
        };
        use crate::owner_authorization_broker::{
            OwnerAuthorityLinearizationV1, OwnerAuthorizationBrokerConfigV1,
        };

        let temp = TempDir::new().unwrap();
        let runtime_root = temp.path().join("authority-runtime");
        std::fs::create_dir(&runtime_root).unwrap();
        let config = test_config(&runtime_root);
        let signer = RealFixtureEd25519Signer::deterministic();
        let keys = real_fixture_keys(&signer);
        let runtime = Arc::new(
            AuthorityRuntime::bootstrap(
                config.clone(),
                Box::new(SoftwareTestProtectedEpochBackend::new()),
            )
            .unwrap(),
        );
        let broker_config = OwnerAuthorizationBrokerConfigV1 {
            root: temp.path().join("authorization-broker"),
            reservation_ttl_ms: 1_000,
            minimum_terminal_retention_ms: 1_000,
        };
        let receipt_crypto: Arc<dyn crate::authority_wal::AuthorityWalRecordCrypto> = Arc::new(
            crate::authority_wal::SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(
                b"authority-receipt-test-only",
            ),
        );
        let protected_journal_head =
            crate::protected_journal_head::SoftwareTestProtectedJournalHeadBackendV1::new()
                .shared();
        let linearization = OwnerAuthorityLinearizationV1::default();
        let (authority_service, mission_provider) = owner_authority_components(
            crate::authority_transport::OwnerAuthorityComponentInputsV1 {
                runtime: Arc::clone(&runtime),
                verification_keys: Arc::new(keys.clone()),
                session_roles: Arc::new(BTreeMap::from([("owner-1".to_string(), Role::Author)])),
                max_future_clock_skew_ms: 10,
                receipt_crypto: Arc::clone(&receipt_crypto),
                broker_config: broker_config.clone(),
                linearization: linearization.clone(),
                protected_journal_head: Arc::clone(&protected_journal_head),
            },
        );
        let ingress_context_digest = test_digest("real-crypto-wire-context");
        let context = MissionServiceTransportContextV1 {
            ingress: MissionServiceIngressV1::Rest,
            transport_session_id: Some("real-rest-session-1".to_string()),
            ingress_context_digest: Some(ingress_context_digest.clone()),
            authority_lease_id: None,
            caller_root: Some("/workspace/m1nd".to_string()),
            route_selector: Some(config.brain_id.clone()),
            actor_brain_id: Some(config.brain_id.clone()),
        };
        let challenge = authority_service
            .issue_session_challenge(
                &context,
                AuthoritySessionChallengeRequestV1 {
                    schema: AUTHORITY_SESSION_CHALLENGE_REQUEST_SCHEMA.to_string(),
                    request_id: "challenge-request-1".to_string(),
                    subject_id: "owner-1".to_string(),
                    key_id: "owner-key-1".to_string(),
                    app_host_identity: "h4nd-fixture".to_string(),
                    nonce: "real-session-nonce-1".to_string(),
                    requested_ttl_ms: 500,
                },
                NOW,
            )
            .unwrap();
        let mut handshake = test_capability(
            &config,
            "runtime.session.handshake",
            challenge.challenge.challenge_digest.clone(),
            &challenge.challenge.core.nonce,
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        cryptographically_sign_test_capability(&mut handshake, &keys, &signer);
        let authenticated = authority_service
            .authenticate_session(
                &context,
                AuthoritySessionAuthenticateRequestV1 {
                    schema: AUTHORITY_SESSION_AUTHENTICATE_REQUEST_SCHEMA.to_string(),
                    request_id: "authenticate-request-1".to_string(),
                    challenge_id: challenge.challenge.core.challenge_id.clone(),
                    capability: handshake.clone(),
                },
                NOW,
            )
            .unwrap();
        assert_eq!(
            authenticated.session.verification_assurance,
            AuthoritySessionVerificationAssuranceV1::ControlVerifiedEd25519
        );
        assert_eq!(
            authenticated.session.session_context_digest,
            ingress_context_digest
        );
        assert_eq!(
            authority_service
                .authenticate_session(
                    &context,
                    AuthoritySessionAuthenticateRequestV1 {
                        schema: AUTHORITY_SESSION_AUTHENTICATE_REQUEST_SCHEMA.to_string(),
                        request_id: "authenticate-replay".to_string(),
                        challenge_id: challenge.challenge.core.challenge_id,
                        capability: handshake,
                    },
                    NOW + 1,
                )
                .unwrap_err()
                .code(),
            "authority_session_challenge_not_pending"
        );

        let mut bootstrap = test_capability(
            &config,
            "brain.bootstrap",
            runtime.status().unwrap().state.record_digest,
            "real-bootstrap-nonce-1",
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        cryptographically_sign_test_capability(&mut bootstrap, &keys, &signer);
        runtime
            .verify_bootstrap(
                &authenticated.session.session_id,
                &authenticated.session.session_context_digest,
                Ingress::Rest,
                &bootstrap,
                &keys,
                NOW,
            )
            .unwrap();

        let intent_digest = test_digest("real-land-intent-digest");
        let authority_decision_digest = test_digest("real-authority-decision");
        let mut land_capability = test_capability(
            &config,
            "mission.service.land",
            intent_digest.clone(),
            "real-land-capability-nonce-1",
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            Some("mission-1"),
            Some("head-1"),
        );
        cryptographically_sign_test_capability(&mut land_capability, &keys, &signer);
        let authorized = authority_service
            .authorize(
                &context,
                AuthorityAuthorizeRequestV1 {
                    schema: AUTHORITY_AUTHORIZE_REQUEST_SCHEMA.to_string(),
                    request_id: "positive-land-authorize-1".to_string(),
                    authority_session_id: Some(authenticated.session.session_id.clone()),
                    authority_session_context_digest: Some(
                        authenticated.session.session_context_digest.clone(),
                    ),
                    target_action: "mission.service.land".to_string(),
                    payload_digest: intent_digest.clone(),
                    requested_effects: catalog_entry("mission.service.land").complete_effects,
                    mission_id: Some("mission-1".to_string()),
                    mission_head_id: Some("head-1".to_string()),
                    input: AuthorityAuthorizeInputV1::PositiveSovereign {
                        capability: Box::new(land_capability),
                        role: Role::Author,
                        capability_kind: CapabilityKind::Human,
                        authority_decision_digest: authority_decision_digest.clone(),
                        applicable_grant_id: None,
                        applicable_tier: None,
                        autonomy_evidence: None,
                    },
                },
                NOW,
            )
            .unwrap();
        assert!(matches!(
            authorized.authorization_receipt.core.authority,
            AuthorizationAuthorityV1::Positive {
                assurance: AuthorityVerificationAssurance::ControlVerifiedEd25519,
                ..
            }
        ));

        let receipt = &authorized.authorization_receipt;
        let mut transaction = m1nd_control::AuthorityTransactionV1::PositiveAuthority(
            m1nd_control::PositiveAuthorityTransactionV1 {
                schema: m1nd_control::POSITIVE_AUTHORITY_TRANSACTION_SCHEMA.to_string(),
                binding: m1nd_control::AuthorityTransactionBindingV1 {
                    transaction_id: "real-land-transaction-1".to_string(),
                    organism_id: receipt.core.organism_id.clone(),
                    brain_id: receipt.core.brain_id.clone(),
                    subject_id: receipt.core.subject_id.clone(),
                    action_id: "land".to_string(),
                    idempotency_key: "real-land-idempotency-1".to_string(),
                    intent_core_ref: format!("intent:{intent_digest}"),
                    intent_digest: intent_digest.clone(),
                    intent_canonicalization_version: m1nd_control::CANONICALIZATION_VERSION
                        .to_string(),
                    capability_id: receipt.core.capability_id.clone(),
                    capability_kind: receipt.core.capability_kind.unwrap(),
                    nonce: "real-land-transaction-nonce-1".to_string(),
                    expected_head_id: Some("head-1".to_string()),
                    expected_active_mode: receipt.core.active_mode,
                    expected_activation_receipt_id: None,
                    expected_constitution_epoch: receipt.core.constitution_epoch,
                    expected_autonomy_epoch: receipt.core.autonomy_epoch,
                    expected_store_epoch: 1,
                    sentinel_verdict_digest: None,
                    authorization_snapshot_digest: receipt.receipt_digest.clone(),
                    issued_at: NOW,
                    expires_at: NOW + 400,
                },
                authority_decision_digest,
                identity_role_binding_digest: test_digest("real-identity-role-binding"),
                required_authority_variant: AuthorityVariant::Human,
                action_policy_registry_digest: receipt.core.policy_registry_digest.clone(),
                classifier_decision_digest: test_digest("real-classifier-decision"),
                expected_pending_red_set_digest: test_digest("real-pending-red-set"),
                expected_red_latch_epoch: 0,
                expected_store_version: 1,
                expected_boundary_version: 1,
                expected_contract_version: 1,
                action_payload_digest: intent_digest,
                issuer: "owner-1".to_string(),
                key_id: "owner-key-1".to_string(),
                algorithm: ED25519_ALGORITHM.to_string(),
                transaction_digest: String::new(),
                signature: OpaqueSignature::new("pending-real-fixture-signature"),
            },
        );
        transaction.seal().unwrap();
        let canonical_transaction = transaction.canonical_signature_payload().unwrap();
        let fixture_signature = m1nd_control::sign_canonical_authority_payload(
            m1nd_control::AUTHORITY_TRANSACTION_SIGNATURE_DOMAIN,
            &canonical_transaction,
            keys.keys.get("owner-key-1").unwrap(),
            &signer,
        )
        .unwrap();
        if let m1nd_control::AuthorityTransactionV1::PositiveAuthority(positive) = &mut transaction
        {
            positive.signature = fixture_signature;
        }
        transaction.validate().unwrap();
        let land = ExternalMissionServiceRequestV1::Land {
            schema: MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA.to_string(),
            request_id: "real-land-wire-request-1".to_string(),
            request: LandRequestV1 {
                schema: LAND_REQUEST_SCHEMA.to_string(),
                brain_id: config.brain_id.clone(),
                mission_id: "mission-1".to_string(),
                expected_head_id: "head-1".to_string(),
                candidate_id: "candidate-1".to_string(),
                expected_candidate_digest: test_digest("real-candidate"),
                expected_store_version: 1,
                idempotency_key: "real-land-idempotency-1".to_string(),
                transaction,
            },
        };
        let land_context = MissionServiceTransportContextV1 {
            authority_lease_id: Some(authorized.authorization_lease_id.clone()),
            ..context
        };
        let provider_with_keys = |verification_keys: VerificationKeyRegistryV1| {
            let status_runtime = Arc::clone(&runtime);
            let current_authority: Arc<crate::mission_service_transport::AuthorityStatusReader> =
                Arc::new(move || status_runtime.status().map_err(|error| error.to_string()));
            crate::mission_service_transport::OwnerBrokerMissionServiceAuthorityProviderV1::from_owner_inputs(
                crate::mission_service_transport::OwnerBrokerAuthorityProviderInputsV1 {
                    broker_config: broker_config.clone(),
                    linearization: linearization.clone(),
                    broker_operation: Arc::new(Mutex::new(())),
                    current_authority,
                    protected_journal_head: Arc::clone(&protected_journal_head),
                    transaction_verification_keys: Arc::new(verification_keys),
                    max_future_clock_skew_ms: 10,
                    receipt_crypto: Arc::clone(&receipt_crypto),
                },
            )
        };

        let mut revoked_keys = keys.clone();
        revoked_keys.registry_epoch += 1;
        let revoked_key = revoked_keys.keys.get_mut("owner-key-1").unwrap();
        revoked_key.status = IdentityStatus::Revoked;
        revoked_key.revoked_at = Some(NOW);
        assert_eq!(
            provider_with_keys(revoked_keys)
                .authenticated_authority(
                    &land_context,
                    &land,
                    &land.authority_object_digest().unwrap(),
                    NOW + 1,
                )
                .unwrap_err()
                .code(),
            "outer_authority_transaction_key_inactive"
        );

        let mut rotated_keys = keys.clone();
        rotated_keys.registry_epoch += 1;
        let mut replacement = rotated_keys.keys["owner-key-1"].clone();
        replacement.key_id = "owner-key-2".to_string();
        replacement.created_at = 50;
        replacement.activated_at = 51;
        let rotated_key = rotated_keys.keys.get_mut("owner-key-1").unwrap();
        rotated_key.status = IdentityStatus::Rotated;
        rotated_key.rotated_at = Some(NOW);
        rotated_key.replacement_key_id = Some(replacement.key_id.clone());
        rotated_keys
            .keys
            .insert(replacement.key_id.clone(), replacement);
        assert_eq!(
            provider_with_keys(rotated_keys)
                .authenticated_authority(
                    &land_context,
                    &land,
                    &land.authority_object_digest().unwrap(),
                    NOW + 1,
                )
                .unwrap_err()
                .code(),
            "outer_authority_transaction_key_inactive"
        );

        let mut signature_tamper = land.clone();
        let ExternalMissionServiceRequestV1::Land { request, .. } = &mut signature_tamper else {
            unreachable!()
        };
        let m1nd_control::AuthorityTransactionV1::PositiveAuthority(positive) =
            &mut request.transaction
        else {
            unreachable!()
        };
        positive.signature = OpaqueSignature::new("tampered-outer-transaction-signature");
        assert_eq!(
            mission_provider
                .authenticated_authority(
                    &land_context,
                    &signature_tamper,
                    &signature_tamper.authority_object_digest().unwrap(),
                    NOW + 1,
                )
                .unwrap_err()
                .code(),
            "outer_authority_transaction_signature_invalid"
        );

        let reserved = mission_provider
            .authenticated_authority(
                &land_context,
                &land,
                &land.authority_object_digest().unwrap(),
                NOW + 1,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            reserved.authorization_snapshot_digest,
            receipt.receipt_digest
        );
        assert_eq!(
            reserved.verified_object_digest,
            land.authority_object_digest().unwrap()
        );
    }

    #[test]
    fn real_positive_authority_transport_reaches_exact_wal_commit_and_consumes_lease() {
        use crate::authority_transport::{
            owner_authority_components, AuthorityAuthorizeInputV1, AuthorityAuthorizeRequestV1,
            AuthoritySessionAuthenticateRequestV1, AuthoritySessionChallengeRequestV1,
            AUTHORITY_AUTHORIZE_REQUEST_SCHEMA, AUTHORITY_SESSION_AUTHENTICATE_REQUEST_SCHEMA,
            AUTHORITY_SESSION_CHALLENGE_REQUEST_SCHEMA,
        };
        use crate::mission_service_tests::{
            advance_to_merge_wait, build_land, config as mission_config, open_service,
            NOW as MISSION_NOW,
        };
        use crate::mission_service_transport::{
            ExternalMissionServiceRequestV1, MissionServiceIngressV1,
            MissionServiceTransportContextV1, MissionServiceTransportFacade,
            MissionServiceTransportResultV1, MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA,
        };
        use crate::owner_authorization_broker::{
            AuthorizationLeaseStateV1, AuthorizationTerminalKindV1, OwnerAuthorityLinearizationV1,
            OwnerAuthorizationBrokerConfigV1, OwnerAuthorizationBrokerV1,
        };

        let authority_temp = TempDir::new().unwrap();
        let runtime_root = authority_temp.path().join("authority-runtime");
        std::fs::create_dir(&runtime_root).unwrap();
        let config = test_config(&runtime_root);
        let signer = RealFixtureEd25519Signer::deterministic();
        let keys = real_fixture_keys(&signer);
        let runtime = Arc::new(
            AuthorityRuntime::bootstrap(
                config.clone(),
                Box::new(SoftwareTestProtectedEpochBackend::new()),
            )
            .unwrap(),
        );
        let broker_config = OwnerAuthorizationBrokerConfigV1 {
            root: authority_temp.path().join("broker"),
            reservation_ttl_ms: 2_000,
            minimum_terminal_retention_ms: 2_000,
        };
        let linearization = OwnerAuthorityLinearizationV1::default();
        let (authority_service, mission_provider) = owner_authority_components(
            crate::authority_transport::OwnerAuthorityComponentInputsV1 {
            runtime: Arc::clone(&runtime),
            verification_keys: Arc::new(keys.clone()),
            session_roles: Arc::new(BTreeMap::from([("owner-1".to_string(), Role::Author)])),
            max_future_clock_skew_ms: 10,
            receipt_crypto: Arc::new(
                crate::authority_wal::SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(
                    b"authority-receipt-test-only",
                ),
            ),
            broker_config: broker_config.clone(),
            linearization: linearization.clone(),
            protected_journal_head: crate::protected_journal_head::SoftwareTestProtectedJournalHeadBackendV1::new()
                .shared(),
            },
        );
        let ingress_context_digest = test_digest("full-land-wire-context");
        let context = MissionServiceTransportContextV1 {
            ingress: MissionServiceIngressV1::Rest,
            transport_session_id: Some("full-land-rest-session".to_string()),
            ingress_context_digest: Some(ingress_context_digest.clone()),
            authority_lease_id: None,
            caller_root: Some("/workspace/m1nd".to_string()),
            route_selector: Some(config.brain_id.clone()),
            actor_brain_id: Some(config.brain_id.clone()),
        };
        let challenge = authority_service
            .issue_session_challenge(
                &context,
                AuthoritySessionChallengeRequestV1 {
                    schema: AUTHORITY_SESSION_CHALLENGE_REQUEST_SCHEMA.to_string(),
                    request_id: "full-land-challenge".to_string(),
                    subject_id: "owner-1".to_string(),
                    key_id: "owner-key-1".to_string(),
                    app_host_identity: "h4nd-full-land-fixture".to_string(),
                    nonce: "full-land-session-nonce".to_string(),
                    requested_ttl_ms: 20_000,
                },
                MISSION_NOW,
            )
            .unwrap();
        let mut handshake = test_capability(
            &config,
            "runtime.session.handshake",
            challenge.challenge.challenge_digest.clone(),
            &challenge.challenge.core.nonce,
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        handshake.issued_at = MISSION_NOW - 1;
        handshake.expires_at = MISSION_NOW + 20_000;
        cryptographically_sign_test_capability_at(&mut handshake, &keys, &signer, MISSION_NOW);
        let session = authority_service
            .authenticate_session(
                &context,
                AuthoritySessionAuthenticateRequestV1 {
                    schema: AUTHORITY_SESSION_AUTHENTICATE_REQUEST_SCHEMA.to_string(),
                    request_id: "full-land-authenticate".to_string(),
                    challenge_id: challenge.challenge.core.challenge_id,
                    capability: handshake,
                },
                MISSION_NOW,
            )
            .unwrap()
            .session;
        let mut bootstrap = test_capability(
            &config,
            "brain.bootstrap",
            runtime.status().unwrap().state.record_digest,
            "full-land-bootstrap-nonce",
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        bootstrap.issued_at = MISSION_NOW;
        bootstrap.expires_at = MISSION_NOW + 20_000;
        cryptographically_sign_test_capability_at(&mut bootstrap, &keys, &signer, MISSION_NOW);
        runtime
            .verify_bootstrap(
                &session.session_id,
                &session.session_context_digest,
                Ingress::Rest,
                &bootstrap,
                &keys,
                MISSION_NOW,
            )
            .unwrap();

        let mission_temp = TempDir::new().unwrap();
        let mut mission = open_service(&mission_temp);
        advance_to_merge_wait(&mut mission, false);
        let (_fixture_authority, mut land_request) =
            build_land(&mission, "full-land-transaction", "full-land-idempotency");
        let intent_digest = land_request.transaction.binding().intent_digest.clone();
        drop(mission);

        let authority_decision_digest = test_digest("full-land-authority-decision");
        let mut capability = test_capability(
            &config,
            "mission.service.land",
            intent_digest.clone(),
            "full-land-capability-nonce",
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            Some(&land_request.mission_id),
            Some(&land_request.expected_head_id),
        );
        capability.issued_at = MISSION_NOW;
        capability.expires_at = MISSION_NOW + 20_000;
        cryptographically_sign_test_capability_at(&mut capability, &keys, &signer, MISSION_NOW);
        let authorized = authority_service
            .authorize(
                &context,
                AuthorityAuthorizeRequestV1 {
                    schema: AUTHORITY_AUTHORIZE_REQUEST_SCHEMA.to_string(),
                    request_id: "full-land-authorize".to_string(),
                    authority_session_id: Some(session.session_id),
                    authority_session_context_digest: Some(session.session_context_digest),
                    target_action: "mission.service.land".to_string(),
                    payload_digest: intent_digest.clone(),
                    requested_effects: catalog_entry("mission.service.land").complete_effects,
                    mission_id: Some(land_request.mission_id.clone()),
                    mission_head_id: Some(land_request.expected_head_id.clone()),
                    input: AuthorityAuthorizeInputV1::PositiveSovereign {
                        capability: Box::new(capability),
                        role: Role::Author,
                        capability_kind: CapabilityKind::Human,
                        authority_decision_digest: authority_decision_digest.clone(),
                        applicable_grant_id: None,
                        applicable_tier: None,
                        autonomy_evidence: None,
                    },
                },
                MISSION_NOW,
            )
            .unwrap();
        let receipt = &authorized.authorization_receipt;
        let m1nd_control::AuthorityTransactionV1::PositiveAuthority(positive) =
            &mut land_request.transaction
        else {
            unreachable!()
        };
        positive.binding.organism_id = receipt.core.organism_id.clone();
        positive.binding.brain_id = receipt.core.brain_id.clone();
        positive.binding.subject_id = receipt.core.subject_id.clone();
        positive.binding.capability_id = receipt.core.capability_id.clone();
        positive.binding.capability_kind = receipt.core.capability_kind.unwrap();
        positive.binding.expected_active_mode = receipt.core.active_mode;
        positive.binding.expected_constitution_epoch = receipt.core.constitution_epoch;
        positive.binding.expected_autonomy_epoch = receipt.core.autonomy_epoch;
        positive.binding.authorization_snapshot_digest = receipt.receipt_digest.clone();
        positive.binding.issued_at = MISSION_NOW;
        positive.binding.expires_at = MISSION_NOW + 10_000;
        positive.authority_decision_digest = authority_decision_digest;
        positive.required_authority_variant = AuthorityVariant::Human;
        positive.action_policy_registry_digest = receipt.core.policy_registry_digest.clone();
        positive.action_payload_digest = intent_digest;
        positive.issuer = "owner-1".to_string();
        positive.key_id = "owner-key-1".to_string();
        positive.algorithm = ED25519_ALGORITHM.to_string();
        positive.signature = OpaqueSignature::new("pending-real-fixture-signature");
        land_request.transaction.seal().unwrap();
        let canonical_transaction = land_request
            .transaction
            .canonical_signature_payload()
            .unwrap();
        let signature = m1nd_control::sign_canonical_authority_payload(
            m1nd_control::AUTHORITY_TRANSACTION_SIGNATURE_DOMAIN,
            &canonical_transaction,
            keys.keys.get("owner-key-1").unwrap(),
            &signer,
        )
        .unwrap();
        let m1nd_control::AuthorityTransactionV1::PositiveAuthority(positive) =
            &mut land_request.transaction
        else {
            unreachable!()
        };
        positive.signature = signature;
        land_request.transaction.validate().unwrap();

        let facade = MissionServiceTransportFacade::open_with_clock_software_test_not_production(
            mission_temp.path(),
            mission_config(),
            mission_provider,
            Arc::new(|| MISSION_NOW + 1),
        )
        .unwrap();
        let land = ExternalMissionServiceRequestV1::Land {
            schema: MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA.to_string(),
            request_id: "full-land-wire-request".to_string(),
            request: land_request,
        };
        let land_context = MissionServiceTransportContextV1 {
            authority_lease_id: Some(authorized.authorization_lease_id.clone()),
            ..context
        };
        let response = facade
            .dispatch_wire_json(&land_context, &serde_json::to_vec(&land).unwrap())
            .unwrap();
        let MissionServiceTransportResultV1::Land { outcome } = response.result else {
            panic!("expected land result")
        };
        assert!(!outcome.deduplicated);
        let broker = OwnerAuthorizationBrokerV1::open(broker_config, linearization).unwrap();
        let lease = broker.lease(&authorized.authorization_lease_id).unwrap();
        assert_eq!(lease.state, AuthorizationLeaseStateV1::Consumed);
        assert_eq!(
            lease.terminal.as_ref().unwrap().kind,
            AuthorizationTerminalKindV1::WalCommitted
        );
        assert_eq!(
            lease
                .terminal
                .as_ref()
                .unwrap()
                .wal_witness
                .as_ref()
                .unwrap()
                .phase,
            m1nd_control::AuthorityWalPhase::Commit
        );
    }

    #[cfg(feature = "serve")]
    #[tokio::test]
    async fn rest_wire_runs_real_session_ceremony_and_positive_authorize() {
        use axum::http::StatusCode;
        use tokio::sync::broadcast;

        use crate::authority_transport::{
            owner_authority_components, AuthorityAuthorizeInputV1, AuthorityAuthorizeRequestV1,
            AuthorityAuthorizeResponseV1, AuthoritySessionAuthenticateRequestV1,
            AuthoritySessionAuthenticateResponseV1, AuthoritySessionChallengeRequestV1,
            AuthoritySessionChallengeResponseV1, AUTHORITY_AUTHORIZE_REQUEST_SCHEMA,
            AUTHORITY_SESSION_AUTHENTICATE_REQUEST_SCHEMA,
            AUTHORITY_SESSION_CHALLENGE_REQUEST_SCHEMA,
        };
        use crate::http_server::{build_router, AppState, SseEvent};
        use crate::owner_authorization_broker::{
            OwnerAuthorityLinearizationV1, OwnerAuthorizationBrokerConfigV1,
        };
        use crate::server::{tool_schemas, McpConfig};

        let temp = TempDir::new().unwrap();
        let runtime_root = temp.path().join("authority-runtime");
        std::fs::create_dir(&runtime_root).unwrap();
        let config = test_config(&runtime_root);
        let signer = RealFixtureEd25519Signer::deterministic();
        let keys = real_fixture_keys(&signer);
        let runtime = Arc::new(
            AuthorityRuntime::bootstrap(
                config.clone(),
                Box::new(SoftwareTestProtectedEpochBackend::new()),
            )
            .unwrap(),
        );
        let (authority_service, _) = owner_authority_components(
            crate::authority_transport::OwnerAuthorityComponentInputsV1 {
            runtime: Arc::clone(&runtime),
            verification_keys: Arc::new(keys.clone()),
            session_roles: Arc::new(BTreeMap::from([("owner-1".to_string(), Role::Author)])),
            max_future_clock_skew_ms: 10,
            receipt_crypto: Arc::new(
                crate::authority_wal::SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(
                    b"authority-receipt-test-only",
                ),
            ),
            broker_config: OwnerAuthorizationBrokerConfigV1 {
                root: temp.path().join("broker"),
                reservation_ttl_ms: 60_000,
                minimum_terminal_retention_ms: 60_000,
            },
            linearization: OwnerAuthorityLinearizationV1::default(),
            protected_journal_head: crate::protected_journal_head::SoftwareTestProtectedJournalHeadBackendV1::new()
                .shared(),
            },
        );

        let owner_root = temp.path().join("owner");
        std::fs::create_dir(&owner_root).unwrap();
        let session = crate::server::McpServer::new(McpConfig {
            graph_source: owner_root.join("graph.json"),
            plasticity_state: owner_root.join("plasticity.json"),
            runtime_dir: Some(owner_root.clone()),
            registry_dir: Some(owner_root.join("registry")),
            ..Default::default()
        })
        .unwrap()
        .into_session_state();
        let (event_tx, _) = broadcast::channel::<SseEvent>(16);
        let app = Arc::new(AppState {
            session: Arc::new(crate::brain_runtime::BrainSessionCell::new(session)),
            tool_schemas_cache: tool_schemas().get("tools").cloned().unwrap_or_default(),
            event_tx,
            event_log_path: None,
            registry_dir: Some(owner_root.join("registry")),
            mcp_sessions: crate::mcp_http::new_mcp_session_registry(),
            project_brains: Arc::new(crate::project_brains::ProjectBrainRegistry::new(
                owner_root.join(crate::project_brains::PROJECT_BRAINS_DIR),
                None,
            )),
            runnerd: Arc::new(crate::runnerd_owner::RunnerdRegistry::default()),
            ui_authority: Arc::new(crate::ui_attestation::UiBundleAttestor::default()),
            mission_service: None,
            external_mutation_service: None,
            authority_service: Some(Arc::clone(&authority_service)),
            autonomy_owner: None,
        });
        let router = build_router(app, false);
        let challenge_request = AuthoritySessionChallengeRequestV1 {
            schema: AUTHORITY_SESSION_CHALLENGE_REQUEST_SCHEMA.to_string(),
            request_id: "wire-challenge-1".to_string(),
            subject_id: "owner-1".to_string(),
            key_id: "owner-key-1".to_string(),
            app_host_identity: "h4nd-rest-fixture".to_string(),
            nonce: "wire-session-nonce-1".to_string(),
            requested_ttl_ms: 60_000,
        };
        let (status, challenge_value) = rest_authority_json(
            &router,
            "/api/authority/session/challenge?brain=brain-1",
            "wire-rest-session-1",
            &challenge_request,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let challenge: AuthoritySessionChallengeResponseV1 =
            serde_json::from_value(challenge_value).unwrap();
        let ceremony_now = challenge.challenge.core.issued_at;
        let mut handshake = test_capability(
            &config,
            "runtime.session.handshake",
            challenge.challenge.challenge_digest.clone(),
            &challenge.challenge.core.nonce,
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        handshake.issued_at = ceremony_now;
        handshake.expires_at = challenge.challenge.core.expires_at;
        cryptographically_sign_test_capability_at(&mut handshake, &keys, &signer, ceremony_now);
        let authenticate_request = AuthoritySessionAuthenticateRequestV1 {
            schema: AUTHORITY_SESSION_AUTHENTICATE_REQUEST_SCHEMA.to_string(),
            request_id: "wire-authenticate-1".to_string(),
            challenge_id: challenge.challenge.core.challenge_id.clone(),
            capability: handshake,
        };
        let (status, authenticated_value) = rest_authority_json(
            &router,
            "/api/authority/session/authenticate?brain=brain-1",
            "wire-rest-session-1",
            &authenticate_request,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let authenticated: AuthoritySessionAuthenticateResponseV1 =
            serde_json::from_value(authenticated_value).unwrap();
        let (status, replay) = rest_authority_json(
            &router,
            "/api/authority/session/authenticate?brain=brain-1",
            "wire-rest-session-1",
            &authenticate_request,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(replay["code"], "authority_session_challenge_not_pending");

        let bootstrap_now = crate::util::now_ms();
        let mut bootstrap = test_capability(
            &config,
            "brain.bootstrap",
            runtime.status().unwrap().state.record_digest,
            "wire-bootstrap-nonce-1",
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        bootstrap.issued_at = bootstrap_now;
        bootstrap.expires_at = bootstrap_now + 60_000;
        cryptographically_sign_test_capability_at(&mut bootstrap, &keys, &signer, bootstrap_now);
        runtime
            .verify_bootstrap(
                &authenticated.session.session_id,
                &authenticated.session.session_context_digest,
                Ingress::Rest,
                &bootstrap,
                &keys,
                bootstrap_now,
            )
            .unwrap();

        let payload_digest = test_digest("wire-positive-land-intent");
        let mut capability = test_capability(
            &config,
            "mission.service.land",
            payload_digest.clone(),
            "wire-positive-land-nonce-1",
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            Some("mission-1"),
            Some("head-1"),
        );
        capability.issued_at = bootstrap_now;
        capability.expires_at = bootstrap_now + 60_000;
        cryptographically_sign_test_capability_at(&mut capability, &keys, &signer, bootstrap_now);
        let authorize_request = AuthorityAuthorizeRequestV1 {
            schema: AUTHORITY_AUTHORIZE_REQUEST_SCHEMA.to_string(),
            request_id: "wire-positive-authorize-1".to_string(),
            authority_session_id: Some(authenticated.session.session_id),
            authority_session_context_digest: Some(authenticated.session.session_context_digest),
            target_action: "mission.service.land".to_string(),
            payload_digest,
            requested_effects: catalog_entry("mission.service.land").complete_effects,
            mission_id: Some("mission-1".to_string()),
            mission_head_id: Some("head-1".to_string()),
            input: AuthorityAuthorizeInputV1::PositiveSovereign {
                capability: Box::new(capability),
                role: Role::Author,
                capability_kind: CapabilityKind::Human,
                authority_decision_digest: test_digest("wire-positive-decision"),
                applicable_grant_id: None,
                applicable_tier: None,
                autonomy_evidence: None,
            },
        };
        let (status, authorized_value) = rest_authority_json(
            &router,
            "/api/authority/authorize?brain=brain-1",
            "wire-rest-session-1",
            &authorize_request,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{authorized_value}");
        let authorized: AuthorityAuthorizeResponseV1 =
            serde_json::from_value(authorized_value).unwrap();
        assert_eq!(
            authorized.authorization_receipt.core.verified_object_digest,
            authorize_request.payload_digest
        );
        assert!(matches!(
            authorized.authorization_receipt.core.authority,
            AuthorizationAuthorityV1::Positive {
                assurance: AuthorityVerificationAssurance::ControlVerifiedEd25519,
                ..
            }
        ));
    }

    #[test]
    fn session_ceremony_rejects_wrong_wire_and_expired_challenge_before_authentication() {
        use crate::authority_transport::{
            owner_authority_components, AuthoritySessionAuthenticateRequestV1,
            AuthoritySessionChallengeRequestV1, AUTHORITY_SESSION_AUTHENTICATE_REQUEST_SCHEMA,
            AUTHORITY_SESSION_CHALLENGE_REQUEST_SCHEMA,
        };
        use crate::mission_service_transport::{
            MissionServiceIngressV1, MissionServiceTransportContextV1,
        };
        use crate::owner_authorization_broker::{
            OwnerAuthorityLinearizationV1, OwnerAuthorizationBrokerConfigV1,
        };

        let temp = TempDir::new().unwrap();
        let runtime_root = temp.path().join("authority-runtime");
        std::fs::create_dir(&runtime_root).unwrap();
        let config = test_config(&runtime_root);
        let signer = RealFixtureEd25519Signer::deterministic();
        let keys = real_fixture_keys(&signer);
        let runtime = Arc::new(
            AuthorityRuntime::bootstrap(
                config.clone(),
                Box::new(SoftwareTestProtectedEpochBackend::new()),
            )
            .unwrap(),
        );
        let (service, _) = owner_authority_components(
            crate::authority_transport::OwnerAuthorityComponentInputsV1 {
            runtime,
            verification_keys: Arc::new(keys.clone()),
            session_roles: Arc::new(BTreeMap::from([("owner-1".to_string(), Role::Author)])),
            max_future_clock_skew_ms: 10,
            receipt_crypto: Arc::new(
                crate::authority_wal::SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(
                    b"authority-receipt-test-only",
                ),
            ),
            broker_config: OwnerAuthorizationBrokerConfigV1 {
                root: temp.path().join("broker"),
                reservation_ttl_ms: 1_000,
                minimum_terminal_retention_ms: 1_000,
            },
            linearization: OwnerAuthorityLinearizationV1::default(),
            protected_journal_head: crate::protected_journal_head::SoftwareTestProtectedJournalHeadBackendV1::new()
                .shared(),
            },
        );
        let context = MissionServiceTransportContextV1 {
            ingress: MissionServiceIngressV1::Rest,
            transport_session_id: Some("wire-1".to_string()),
            ingress_context_digest: Some(test_digest("wire-1")),
            authority_lease_id: None,
            caller_root: None,
            route_selector: Some(config.brain_id.clone()),
            actor_brain_id: Some(config.brain_id.clone()),
        };
        let challenge = service
            .issue_session_challenge(
                &context,
                AuthoritySessionChallengeRequestV1 {
                    schema: AUTHORITY_SESSION_CHALLENGE_REQUEST_SCHEMA.to_string(),
                    request_id: "short-challenge".to_string(),
                    subject_id: "owner-1".to_string(),
                    key_id: "owner-key-1".to_string(),
                    app_host_identity: "h4nd-fixture".to_string(),
                    nonce: "short-challenge-nonce".to_string(),
                    requested_ttl_ms: 1,
                },
                NOW,
            )
            .unwrap();
        let mut capability = test_capability(
            &config,
            "runtime.session.handshake",
            challenge.challenge.challenge_digest.clone(),
            &challenge.challenge.core.nonce,
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        cryptographically_sign_test_capability(&mut capability, &keys, &signer);
        let wrong_context = MissionServiceTransportContextV1 {
            transport_session_id: Some("wire-2".to_string()),
            ingress_context_digest: Some(test_digest("wire-2")),
            ..context.clone()
        };
        assert_eq!(
            service
                .authenticate_session(
                    &wrong_context,
                    AuthoritySessionAuthenticateRequestV1 {
                        schema: AUTHORITY_SESSION_AUTHENTICATE_REQUEST_SCHEMA.to_string(),
                        request_id: "wrong-wire".to_string(),
                        challenge_id: challenge.challenge.core.challenge_id.clone(),
                        capability: capability.clone(),
                    },
                    NOW,
                )
                .unwrap_err()
                .code(),
            "authority_session_transport_mismatch"
        );
        assert_eq!(
            service
                .authenticate_session(
                    &context,
                    AuthoritySessionAuthenticateRequestV1 {
                        schema: AUTHORITY_SESSION_AUTHENTICATE_REQUEST_SCHEMA.to_string(),
                        request_id: "expired".to_string(),
                        challenge_id: challenge.challenge.core.challenge_id,
                        capability,
                    },
                    NOW + 1,
                )
                .unwrap_err()
                .code(),
            "authority_session_challenge_expired"
        );
    }

    #[test]
    fn safety_path_is_disjoint_negative_only_and_replay_protected_while_frozen() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let runtime = AuthorityRuntime::bootstrap_software_test(
            config.clone(),
            SoftwareTestProtectedEpochBackend::new(),
            Some(Box::new(SoftwareTestSafetyVerifier)),
        )
        .unwrap();
        let payload = test_digest("freeze-payload");
        let attempt = safety_attempt(
            &config,
            "safety.freeze_issuance",
            payload.clone(),
            "safety-once",
        );
        let request = AuthorityAuthorizationRequestV1 {
            session_id: None,
            session_context_digest: None,
            transport_session_id: "transport-session-1".to_string(),
            ingress_context_digest: test_digest("ingress-context"),
            ingress: Ingress::Recovery,
            action: attempt.core.action.clone(),
            payload_digest: payload,
            requested_effects: attempt.core.negative_effects.clone(),
            mission_id: None,
            mission_head_id: None,
            now_ms: NOW,
        };
        let receipt = runtime
            .authorize_mutation(
                request.clone(),
                AuthorityInputV1::Safety { attempt: &attempt },
            )
            .unwrap();
        assert!(matches!(
            receipt.core.authority,
            AuthorizationAuthorityV1::SafetyActuator {
                assurance: SafetyVerifierAssurance::SoftwareTestOnlyNotProven
            }
        ));
        assert!(receipt
            .core
            .complete_effects
            .iter()
            .all(|effect| effect.is_negative_safety()));
        assert!(runtime.status().unwrap().state.core.issuance_frozen);
        let replay = runtime
            .authorize_mutation(request, AuthorityInputV1::Safety { attempt: &attempt })
            .unwrap_err();
        assert!(matches!(
            replay,
            AuthorityRuntimeError::Replay(ReplayLedgerError::Replay { .. })
        ));

        let keys = test_keys();
        let positive = test_capability(
            &config,
            "safety.freeze_issuance",
            attempt.core.payload_digest.clone(),
            "wrong-positive-path",
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        let safety_requires_actuator = runtime
            .authorize_mutation(
                AuthorityAuthorizationRequestV1 {
                    session_id: None,
                    session_context_digest: None,
                    transport_session_id: "transport-session-1".to_string(),
                    ingress_context_digest: test_digest("ingress-context"),
                    ingress: Ingress::Recovery,
                    action: attempt.core.action.clone(),
                    payload_digest: attempt.core.payload_digest.clone(),
                    requested_effects: attempt.core.negative_effects.clone(),
                    mission_id: None,
                    mission_head_id: None,
                    now_ms: NOW,
                },
                AuthorityInputV1::Positive {
                    capability: &positive,
                    keys: &keys,
                },
            )
            .unwrap_err();
        assert!(matches!(
            safety_requires_actuator,
            AuthorityRuntimeError::SafetyAuthorityRequired
        ));
    }

    #[test]
    fn durable_restart_matches_state_journal_replay_and_owner_lease_is_unique() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let keys = test_keys();
        let backend = SoftwareTestProtectedEpochBackend::new();
        let runtime =
            AuthorityRuntime::bootstrap_software_test(config.clone(), backend.clone(), None)
                .unwrap();
        let busy = AuthorityRuntime::open_software_test(config.clone(), backend.clone(), None);
        let busy_error = match busy {
            Ok(_) => panic!("second owner lease unexpectedly opened"),
            Err(error) => error,
        };
        assert!(matches!(
            busy_error,
            AuthorityRuntimeError::OwnerLeaseBusy { .. }
        ));

        let session = authenticate_test_session(&runtime, &config, &keys, "restart");
        let expected =
            verify_test_bootstrap(&runtime, &config, &keys, &session, "bootstrap-restart");
        drop(runtime);
        let reopened = AuthorityRuntime::open_software_test(config, backend, None).unwrap();
        assert_eq!(reopened.status().unwrap().state, expected);
    }

    #[test]
    fn authority_root_symlink_or_rename_replacement_fails_closed() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let real_root = temp.path().join("real-authority-runtime");
        fs::create_dir(&real_root).unwrap();
        let linked_root = temp.path().join("linked-authority-runtime");
        symlink(&real_root, &linked_root).unwrap();
        let linked_error = match AuthorityRuntime::bootstrap_software_test(
            test_config(&linked_root),
            SoftwareTestProtectedEpochBackend::new(),
            None,
        ) {
            Ok(_) => panic!("symlink authority root unexpectedly bootstrapped"),
            Err(error) => error,
        };
        assert!(matches!(
            linked_error,
            AuthorityRuntimeError::InvalidContract { .. }
        ));
        assert_eq!(fs::read_dir(&real_root).unwrap().count(), 0);

        let root = temp.path().join("authority-runtime");
        let displaced = temp.path().join("authority-runtime.displaced");
        let config = test_config(&root);
        let backend = SoftwareTestProtectedEpochBackend::new();
        let runtime =
            AuthorityRuntime::bootstrap_software_test(config.clone(), backend.clone(), None)
                .unwrap();

        fs::rename(&root, &displaced).unwrap();
        fs::create_dir(&root).unwrap();

        let status_error = runtime
            .status()
            .expect_err("live owner must detect a replaced authority root");
        assert!(matches!(
            status_error,
            AuthorityRuntimeError::RollbackDetected { .. }
        ));

        let second = AuthorityRuntime::open_software_test(config, backend, None);
        let second_error = match second {
            Ok(_) => panic!("replacement root minted a second in-process authority owner"),
            Err(error) => error,
        };
        assert!(matches!(
            second_error,
            AuthorityRuntimeError::OwnerLeaseBusy { .. }
        ));
        assert_eq!(
            fs::read_dir(&root).unwrap().count(),
            0,
            "neither owner may populate the replacement authority root"
        );
        assert!(displaced.join(STATE_FILE_NAME).is_file());
        assert!(displaced.join(JOURNAL_FILE_NAME).is_file());
        assert!(displaced.join(REPLAY_FILE_NAME).is_file());
    }

    #[test]
    fn missing_corrupt_activation_conflict_or_orphan_temp_project_fail_closed_never_full() {
        let missing_temp = TempDir::new().unwrap();
        let missing_config = test_config(missing_temp.path());
        let missing = AuthorityRuntime::open_software_test(
            missing_config,
            SoftwareTestProtectedEpochBackend::new(),
            None,
        );
        let missing_error = match missing {
            Ok(_) => panic!("missing state unexpectedly opened"),
            Err(error) => error,
        };
        let projection = FailClosedAuthorityProjectionV1::from_open_error(&missing_error);
        assert_eq!(projection.active_mode, ActiveMode::HumanGated);
        assert!(projection.issuance_frozen);
        assert!(!projection.full_autonomy);
        assert!(!projection.may_authorize_positive);

        let corrupt_temp = TempDir::new().unwrap();
        let corrupt_config = test_config(corrupt_temp.path());
        let corrupt_backend = SoftwareTestProtectedEpochBackend::new();
        let corrupt_runtime = AuthorityRuntime::bootstrap_software_test(
            corrupt_config.clone(),
            corrupt_backend.clone(),
            None,
        )
        .unwrap();
        drop(corrupt_runtime);
        fs::write(
            corrupt_config.root.join(STATE_FILE_NAME),
            b"{not-valid-json",
        )
        .unwrap();
        let corrupt = AuthorityRuntime::open_software_test(corrupt_config, corrupt_backend, None);
        let corrupt_error = match corrupt {
            Ok(_) => panic!("corrupt state unexpectedly opened"),
            Err(error) => error,
        };
        assert!(!FailClosedAuthorityProjectionV1::from_open_error(&corrupt_error).full_autonomy);

        let conflict_temp = TempDir::new().unwrap();
        let conflict_config = test_config(conflict_temp.path());
        let conflict_backend = SoftwareTestProtectedEpochBackend::new();
        let conflict_runtime = AuthorityRuntime::bootstrap_software_test(
            conflict_config.clone(),
            conflict_backend.clone(),
            None,
        )
        .unwrap();
        let mut conflict_state = conflict_runtime.status().unwrap().state;
        drop(conflict_runtime);
        conflict_state.core.active_mode = ActiveMode::FullAutonomy;
        conflict_state.seal().unwrap();
        conflict_backend.force_snapshot(Some(conflict_state.protected_snapshot()));
        fs::write(
            conflict_config.root.join(STATE_FILE_NAME),
            canonical_json(&conflict_state).unwrap(),
        )
        .unwrap();
        let conflict =
            AuthorityRuntime::open_software_test(conflict_config, conflict_backend, None);
        let conflict_error = match conflict {
            Ok(_) => panic!("activation conflict unexpectedly opened"),
            Err(error) => error,
        };
        assert!(matches!(
            conflict_error,
            AuthorityRuntimeError::ActivationConflict { .. }
        ));
        assert!(!FailClosedAuthorityProjectionV1::from_open_error(&conflict_error).full_autonomy);

        let orphan_temp = TempDir::new().unwrap();
        let orphan_config = test_config(orphan_temp.path());
        let orphan_backend = SoftwareTestProtectedEpochBackend::new();
        let orphan_runtime = AuthorityRuntime::bootstrap_software_test(
            orphan_config.clone(),
            orphan_backend.clone(),
            None,
        )
        .unwrap();
        drop(orphan_runtime);
        fs::write(
            orphan_config.root.join(format!(
                ".{STATE_FILE_NAME}.tmp.{}.fixture",
                std::process::id()
            )),
            b"orphan",
        )
        .unwrap();
        let orphan = AuthorityRuntime::open_software_test(orphan_config, orphan_backend, None);
        let orphan_error = match orphan {
            Ok(_) => panic!("orphan atomic temp unexpectedly opened"),
            Err(error) => error,
        };
        assert!(matches!(
            orphan_error,
            AuthorityRuntimeError::CorruptState { .. }
        ));
    }

    #[test]
    fn rollback_torn_journal_and_corrupt_replay_are_detected_on_recovery_open() {
        let rollback_temp = TempDir::new().unwrap();
        let rollback_config = test_config(rollback_temp.path());
        let rollback_backend = SoftwareTestProtectedEpochBackend::new();
        let rollback_runtime = AuthorityRuntime::bootstrap_software_test(
            rollback_config.clone(),
            rollback_backend.clone(),
            None,
        )
        .unwrap();
        let old_snapshot = rollback_backend.snapshot();
        let keys = test_keys();
        authenticate_test_session(&rollback_runtime, &rollback_config, &keys, "rollback");
        drop(rollback_runtime);
        rollback_backend.force_snapshot(old_snapshot);
        let rollback =
            AuthorityRuntime::open_software_test(rollback_config, rollback_backend, None);
        let rollback_error = match rollback {
            Ok(_) => panic!("rolled-back protected epoch unexpectedly opened"),
            Err(error) => error,
        };
        assert!(matches!(
            rollback_error,
            AuthorityRuntimeError::RollbackDetected { .. }
        ));

        let journal_temp = TempDir::new().unwrap();
        let journal_config = test_config(journal_temp.path());
        let journal_backend = SoftwareTestProtectedEpochBackend::new();
        let journal_runtime = AuthorityRuntime::bootstrap_software_test(
            journal_config.clone(),
            journal_backend.clone(),
            None,
        )
        .unwrap();
        drop(journal_runtime);
        OpenOptions::new()
            .append(true)
            .open(journal_config.root.join(JOURNAL_FILE_NAME))
            .unwrap()
            .write_all(b"{torn")
            .unwrap();
        let journal = AuthorityRuntime::open_software_test(journal_config, journal_backend, None);
        let journal_error = match journal {
            Ok(_) => panic!("torn journal unexpectedly opened"),
            Err(error) => error,
        };
        assert!(matches!(
            journal_error,
            AuthorityRuntimeError::CorruptJournal { .. }
        ));

        let replay_temp = TempDir::new().unwrap();
        let replay_config = test_config(replay_temp.path());
        let replay_backend = SoftwareTestProtectedEpochBackend::new();
        let replay_runtime = AuthorityRuntime::bootstrap_software_test(
            replay_config.clone(),
            replay_backend.clone(),
            None,
        )
        .unwrap();
        drop(replay_runtime);
        fs::write(replay_config.root.join(REPLAY_FILE_NAME), b"invalid\n").unwrap();
        let replay = AuthorityRuntime::open_software_test(replay_config, replay_backend, None);
        let replay_error = match replay {
            Ok(_) => panic!("corrupt replay ledger unexpectedly opened"),
            Err(error) => error,
        };
        assert!(matches!(
            replay_error,
            AuthorityRuntimeError::CorruptReplay { .. }
        ));
    }

    #[derive(Clone)]
    struct AdvanceThenFailBackend {
        inner: SoftwareTestProtectedEpochBackend,
    }

    #[derive(Clone)]
    struct FailWithoutAdvanceBackend {
        inner: SoftwareTestProtectedEpochBackend,
    }

    impl ProtectedEpochBackend for FailWithoutAdvanceBackend {
        fn assurance(&self) -> ProtectedEpochAssurance {
            ProtectedEpochAssurance::SoftwareTestOnlyNotProven
        }

        fn read_latest(&self) -> Result<Option<ProtectedEpochSnapshotV1>, String> {
            self.inner.read_latest()
        }

        fn compare_and_advance(
            &mut self,
            _expected: Option<&ProtectedEpochSnapshotV1>,
            _next: &ProtectedEpochSnapshotV1,
        ) -> Result<(), String> {
            Err("fault injection before protected CAS".to_string())
        }
    }

    impl ProtectedEpochBackend for AdvanceThenFailBackend {
        fn assurance(&self) -> ProtectedEpochAssurance {
            ProtectedEpochAssurance::SoftwareTestOnlyNotProven
        }

        fn read_latest(&self) -> Result<Option<ProtectedEpochSnapshotV1>, String> {
            self.inner.read_latest()
        }

        fn compare_and_advance(
            &mut self,
            expected: Option<&ProtectedEpochSnapshotV1>,
            next: &ProtectedEpochSnapshotV1,
        ) -> Result<(), String> {
            self.inner.compare_and_advance(expected, next)?;
            Err("fault injection after protected CAS".to_string())
        }
    }

    #[test]
    fn backend_error_without_cas_rolls_back_exact_prepared_bootstrap_tails() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let shared = SoftwareTestProtectedEpochBackend::new();
        let result = AuthorityRuntime::bootstrap_with_components(
            config.clone(),
            Box::new(FailWithoutAdvanceBackend {
                inner: shared.clone(),
            }),
            Box::new(SoftwareTestPositiveAuthorityVerifier),
            None,
            None,
            None,
        );
        assert!(matches!(
            result,
            Err(AuthorityRuntimeError::ProtectedEpoch { .. })
        ));
        assert!(shared.snapshot().is_none());
        assert!(config.root.join(TRANSITION_DESCRIPTOR_FILE_NAME).exists());
        let recovered =
            AuthorityRuntime::bootstrap_software_test(config.clone(), shared.clone(), None)
                .unwrap();
        let state = recovered.status().unwrap().state;
        assert_eq!(state.core.protected_epoch, 1);
        assert_eq!(state.core.journal_sequence, 1);
        assert_eq!(state.core.replay_sequence, 0);
        assert_eq!(shared.snapshot(), Some(state.protected_snapshot()));
        assert!(!config.root.join(TRANSITION_DESCRIPTOR_FILE_NAME).exists());
    }

    #[test]
    fn backend_error_after_effective_cas_forward_completes_exact_bootstrap_descriptor() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let shared = SoftwareTestProtectedEpochBackend::new();
        let result = AuthorityRuntime::bootstrap_with_components(
            config.clone(),
            Box::new(AdvanceThenFailBackend {
                inner: shared.clone(),
            }),
            Box::new(SoftwareTestPositiveAuthorityVerifier),
            None,
            None,
            None,
        );
        let error = match result {
            Ok(_) => panic!("fault-injected bootstrap unexpectedly succeeded"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            AuthorityRuntimeError::ProtectedEpoch { .. }
        ));
        assert!(shared.snapshot().is_some());
        assert!(!config.root.join(STATE_FILE_NAME).exists());
        let reopened = AuthorityRuntime::open_software_test(config.clone(), shared, None).unwrap();
        let state = reopened.status().unwrap().state;
        assert_eq!(state.core.protected_epoch, 1);
        assert_eq!(state.core.journal_sequence, 1);
        assert!(!config.root.join(TRANSITION_DESCRIPTOR_FILE_NAME).exists());
    }

    #[test]
    fn bootstrap_recovers_old_or_new_at_every_prepared_boundary() {
        let points = [
            TransitionFaultPoint::Descriptor,
            TransitionFaultPoint::Replay,
            TransitionFaultPoint::Journal,
            TransitionFaultPoint::ProtectedCas,
            TransitionFaultPoint::State,
            TransitionFaultPoint::Cleanup,
        ];
        for point in points {
            let temp = TempDir::new().unwrap();
            let config = test_config(temp.path());
            let backend = SoftwareTestProtectedEpochBackend::new();
            let failed = AuthorityRuntime::bootstrap_software_test_with_fault(
                config.clone(),
                backend.clone(),
                None,
                point,
            );
            let error = match failed {
                Ok(_) => panic!("bootstrap fault {point:?} unexpectedly succeeded"),
                Err(error) => error,
            };
            assert!(matches!(error, AuthorityRuntimeError::FaultInjected { .. }));

            let recovered = if matches!(
                point,
                TransitionFaultPoint::Descriptor
                    | TransitionFaultPoint::Replay
                    | TransitionFaultPoint::Journal
            ) {
                assert!(backend.snapshot().is_none());
                AuthorityRuntime::bootstrap_software_test(config.clone(), backend.clone(), None)
                    .unwrap()
            } else {
                assert!(backend.snapshot().is_some());
                AuthorityRuntime::open_software_test(config.clone(), backend.clone(), None).unwrap()
            };
            let state = recovered.status().unwrap().state;
            assert_eq!(state.core.revision, 0, "fault point {point:?}");
            assert_eq!(state.core.protected_epoch, 1, "fault point {point:?}");
            assert_eq!(state.core.journal_sequence, 1, "fault point {point:?}");
            assert_eq!(state.core.replay_sequence, 0, "fault point {point:?}");
            assert_eq!(backend.snapshot(), Some(state.protected_snapshot()));
            assert!(!config.root.join(TRANSITION_DESCRIPTOR_FILE_NAME).exists());
        }
    }

    #[test]
    fn runtime_transition_recovers_old_or_new_at_every_prepared_boundary() {
        let points = [
            TransitionFaultPoint::Descriptor,
            TransitionFaultPoint::Replay,
            TransitionFaultPoint::Journal,
            TransitionFaultPoint::ProtectedCas,
            TransitionFaultPoint::State,
            TransitionFaultPoint::Cleanup,
        ];
        for (index, point) in points.into_iter().enumerate() {
            let temp = TempDir::new().unwrap();
            let config = test_config(temp.path());
            let keys = test_keys();
            let backend = SoftwareTestProtectedEpochBackend::new();
            let runtime =
                AuthorityRuntime::bootstrap_software_test(config.clone(), backend.clone(), None)
                    .unwrap();
            let session =
                authenticate_test_session(&runtime, &config, &keys, &format!("fault-{index}"));
            verify_test_bootstrap(
                &runtime,
                &config,
                &keys,
                &session,
                &format!("bootstrap-fault-{index}"),
            );
            let prior = runtime.status().unwrap().state;
            let payload = test_digest(&format!("fault-payload-{index}"));
            let capability = test_capability(
                &config,
                "system_blocks.ratify",
                payload.clone(),
                &format!("fault-capability-{index}"),
                AuthorityVariant::Human,
                ActiveMode::HumanGated,
                None,
                None,
            );
            runtime.set_transition_fault(point);
            let error = runtime
                .authorize_mutation(
                    positive_request("system_blocks.ratify", Ingress::Mcp, payload, &session),
                    AuthorityInputV1::Positive {
                        capability: &capability,
                        keys: &keys,
                    },
                )
                .unwrap_err();
            assert!(matches!(error, AuthorityRuntimeError::FaultInjected { .. }));
            drop(runtime);

            let reopened =
                AuthorityRuntime::open_software_test(config.clone(), backend.clone(), None)
                    .unwrap();
            let recovered = reopened.status().unwrap().state;
            if matches!(
                point,
                TransitionFaultPoint::Descriptor
                    | TransitionFaultPoint::Replay
                    | TransitionFaultPoint::Journal
            ) {
                assert_eq!(recovered, prior, "fault point {point:?}");
            } else {
                assert_eq!(
                    recovered.core.revision,
                    prior.core.revision + 1,
                    "fault point {point:?}"
                );
                assert_eq!(
                    recovered.core.replay_sequence,
                    prior.core.replay_sequence + 1,
                    "fault point {point:?}"
                );
                assert_eq!(
                    recovered.core.journal_sequence,
                    prior.core.journal_sequence + 1,
                    "fault point {point:?}"
                );
                assert_eq!(
                    recovered.core.protected_epoch,
                    prior.core.protected_epoch + 1,
                    "fault point {point:?}"
                );
            }
            assert_eq!(backend.snapshot(), Some(recovered.protected_snapshot()));
            assert!(!config.root.join(TRANSITION_DESCRIPTOR_FILE_NAME).exists());
        }
    }

    #[test]
    fn corrupt_descriptor_and_unbound_tail_never_trigger_inferred_recovery() {
        let descriptor_temp = TempDir::new().unwrap();
        let descriptor_config = test_config(descriptor_temp.path());
        let descriptor_backend = SoftwareTestProtectedEpochBackend::new();
        let failed = AuthorityRuntime::bootstrap_software_test_with_fault(
            descriptor_config.clone(),
            descriptor_backend.clone(),
            None,
            TransitionFaultPoint::Descriptor,
        );
        assert!(matches!(
            failed,
            Err(AuthorityRuntimeError::FaultInjected { .. })
        ));
        let descriptor_path = descriptor_config.root.join(TRANSITION_DESCRIPTOR_FILE_NAME);
        let mut descriptor_json: serde_json::Value =
            serde_json::from_slice(&fs::read(&descriptor_path).unwrap()).unwrap();
        descriptor_json["descriptor_digest"] = serde_json::Value::String("0".repeat(64));
        fs::write(&descriptor_path, canonical_json(&descriptor_json).unwrap()).unwrap();
        let before_descriptor = fs::read(&descriptor_path).unwrap();
        let corrupt =
            AuthorityRuntime::bootstrap_software_test(descriptor_config, descriptor_backend, None);
        let corrupt_error = match corrupt {
            Ok(_) => panic!("corrupt descriptor unexpectedly recovered"),
            Err(error) => error,
        };
        assert!(matches!(
            corrupt_error,
            AuthorityRuntimeError::CorruptTransitionDescriptor { .. }
        ));
        assert_eq!(fs::read(&descriptor_path).unwrap(), before_descriptor);

        let tail_temp = TempDir::new().unwrap();
        let tail_config = test_config(tail_temp.path());
        let keys = test_keys();
        let tail_backend = SoftwareTestProtectedEpochBackend::new();
        let runtime = AuthorityRuntime::bootstrap_software_test(
            tail_config.clone(),
            tail_backend.clone(),
            None,
        )
        .unwrap();
        let session = authenticate_test_session(&runtime, &tail_config, &keys, "unbound-tail");
        verify_test_bootstrap(
            &runtime,
            &tail_config,
            &keys,
            &session,
            "bootstrap-unbound-tail",
        );
        let payload = test_digest("unbound-tail-payload");
        let capability = test_capability(
            &tail_config,
            "system_blocks.ratify",
            payload.clone(),
            "unbound-tail-capability",
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        runtime.set_transition_fault(TransitionFaultPoint::Replay);
        assert!(matches!(
            runtime.authorize_mutation(
                positive_request("system_blocks.ratify", Ingress::Mcp, payload, &session,),
                AuthorityInputV1::Positive {
                    capability: &capability,
                    keys: &keys,
                },
            ),
            Err(AuthorityRuntimeError::FaultInjected { .. })
        ));
        drop(runtime);
        let replay_path = tail_config.root.join(REPLAY_FILE_NAME);
        OpenOptions::new()
            .append(true)
            .open(&replay_path)
            .unwrap()
            .write_all(b"unbound-tail")
            .unwrap();
        let replay_len = fs::metadata(&replay_path).unwrap().len();
        let unbound = AuthorityRuntime::open_software_test(tail_config, tail_backend, None);
        let unbound_error = match unbound {
            Ok(_) => panic!("unbound replay tail unexpectedly truncated"),
            Err(error) => error,
        };
        assert!(matches!(
            unbound_error,
            AuthorityRuntimeError::CorruptTransitionDescriptor { .. }
        ));
        assert_eq!(fs::metadata(&replay_path).unwrap().len(), replay_len);
    }

    #[test]
    fn exact_valid_tail_without_prepared_descriptor_is_never_inferred_as_success() {
        let replay_temp = TempDir::new().unwrap();
        let replay_config = test_config(replay_temp.path());
        let replay_backend = SoftwareTestProtectedEpochBackend::new();
        let replay_runtime = AuthorityRuntime::bootstrap_software_test(
            replay_config.clone(),
            replay_backend.clone(),
            None,
        )
        .unwrap();
        {
            let mut inner = replay_runtime.inner.lock();
            inner
                .replay
                .consume(
                    &ReplayClaimV1 {
                        schema: REPLAY_CLAIM_SCHEMA.to_string(),
                        namespace: "unbound-valid-tail".to_string(),
                        issuer_subject_id: "owner-1".to_string(),
                        key_id: "owner-key-1".to_string(),
                        subject_id: "owner-1".to_string(),
                        nonce: "unbound-valid-replay".to_string(),
                        object_digest: test_digest("unbound-valid-object"),
                        issued_at: NOW - 1,
                        expires_at: NOW + 100,
                    },
                    NOW,
                    0,
                )
                .unwrap();
            inner.replay.append_pending().unwrap();
        }
        drop(replay_runtime);
        assert!(!replay_config
            .root
            .join(TRANSITION_DESCRIPTOR_FILE_NAME)
            .exists());
        let replay_open = AuthorityRuntime::open_software_test(replay_config, replay_backend, None);
        let replay_error = match replay_open {
            Ok(_) => panic!("descriptor-free valid replay tail was inferred as committed"),
            Err(error) => error,
        };
        assert!(matches!(
            replay_error,
            AuthorityRuntimeError::RollbackDetected { .. }
        ));

        let journal_temp = TempDir::new().unwrap();
        let journal_config = test_config(journal_temp.path());
        let journal_backend = SoftwareTestProtectedEpochBackend::new();
        let journal_runtime = AuthorityRuntime::bootstrap_software_test(
            journal_config.clone(),
            journal_backend.clone(),
            None,
        )
        .unwrap();
        {
            let mut inner = journal_runtime.inner.lock();
            let record = inner
                .journal
                .prepare(
                    AuthorityJournalEventKind::PositiveMutationAuthorized,
                    test_digest("unbound-valid-journal"),
                    2,
                    NOW,
                )
                .unwrap();
            inner.journal.append_prepared(&record).unwrap();
        }
        drop(journal_runtime);
        assert!(!journal_config
            .root
            .join(TRANSITION_DESCRIPTOR_FILE_NAME)
            .exists());
        let journal_open =
            AuthorityRuntime::open_software_test(journal_config, journal_backend, None);
        let journal_error = match journal_open {
            Ok(_) => panic!("descriptor-free valid journal tail was inferred as committed"),
            Err(error) => error,
        };
        assert!(matches!(
            journal_error,
            AuthorityRuntimeError::CorruptJournal { .. }
        ));
    }

    #[test]
    fn owner_serial_allows_exactly_one_concurrent_replay_claim() {
        let temp = TempDir::new().unwrap();
        let config = test_config(temp.path());
        let keys = test_keys();
        let runtime = Arc::new(
            AuthorityRuntime::bootstrap_software_test(
                config.clone(),
                SoftwareTestProtectedEpochBackend::new(),
                None,
            )
            .unwrap(),
        );
        let session = authenticate_test_session(&runtime, &config, &keys, "concurrency");
        verify_test_bootstrap(&runtime, &config, &keys, &session, "bootstrap-concurrency");
        let payload = test_digest("concurrent-payload");
        let capability = test_capability(
            &config,
            "system_blocks.ratify",
            payload.clone(),
            "same-concurrent-nonce",
            AuthorityVariant::Human,
            ActiveMode::HumanGated,
            None,
            None,
        );
        let barrier = Arc::new(Barrier::new(3));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let runtime = Arc::clone(&runtime);
            let barrier = Arc::clone(&barrier);
            let config_keys = keys.clone();
            let capability = capability.clone();
            let request = positive_request(
                "system_blocks.ratify",
                Ingress::Mcp,
                payload.clone(),
                &session,
            );
            handles.push(thread::spawn(move || {
                barrier.wait();
                runtime.authorize_mutation(
                    request,
                    AuthorityInputV1::Positive {
                        capability: &capability,
                        keys: &config_keys,
                    },
                )
            }));
        }
        barrier.wait();
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(
                    result,
                    Err(AuthorityRuntimeError::Replay(
                        ReplayLedgerError::Replay { .. }
                    ))
                ))
                .count(),
            1
        );
        assert_eq!(runtime.status().unwrap().state.core.replay_sequence, 3);
    }
}
