//! Durable owner outbox and runner inbox for execution dispatch.
//!
//! The signed [`ExecutionDispatchV1`] is immutable and always remains in
//! `INTENT`; operational progress is stored beside it.  The runner's durable
//! `CLAIMED` snapshot is the at-most-once spawn linearization point.  Recovery
//! therefore never grants a second spawn permit for a claimed execution.
//!
//! This module deliberately persists no mission letters and is deliberately
//! not exported by `lib.rs`.  Its reconciliation APIs return typed actions for
//! the MissionService integration layer.  Local journal durability and Unix
//! single-writer behavior are implemented here; transport and real process
//! runner wiring remain outside this slice.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use m1nd_control::{
    ExecutionDispatchAckV1, ExecutionDispatchState, ExecutionDispatchV1, ExecutionOutcome,
    ExecutionResultV1, MissionContractError, MissionHeadContext, MissionHeadSnapshot, MissionState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const OWNER_DISPATCH_ENTRY_SCHEMA: &str = "m1nd-owner-dispatch-entry-v1";
pub const RUNNER_INBOX_ENTRY_SCHEMA: &str = "m1nd-runner-inbox-entry-v1";
pub const EXECUTION_MISSION_HEAD_SCHEMA: &str = "m1nd-execution-mission-head-v1";
pub const PROCESS_CLAIM_SCHEMA: &str = "m1nd-process-claim-v1";
pub const DISPATCH_JOURNAL_RECORD_SCHEMA: &str = "m1nd-execution-dispatch-journal-record-v1";

const DISPATCH_JOURNAL_DIGEST_DOMAIN: &str = "m1nd-execution-dispatch-journal-record-v1";
const PROCESS_CLAIM_DIGEST_DOMAIN: &str = "m1nd-process-claim-v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionMissionHeadV1 {
    pub schema: String,
    pub head_id: String,
    pub state: MissionState,
    pub iteration_id: u64,
    pub packet_digest: String,
}

impl ExecutionMissionHeadV1 {
    fn validate_against(&self, dispatch: &ExecutionDispatchV1) -> Result<(), String> {
        if self.schema != EXECUTION_MISSION_HEAD_SCHEMA {
            return Err(format!(
                "unsupported executing-head schema '{}'",
                self.schema
            ));
        }
        validate_identifier("executing_head.head_id", &self.head_id)?;
        if self.state != MissionState::Executing {
            return Err("executing head must have state EXECUTING".to_string());
        }
        if self.iteration_id != dispatch.iteration_id {
            return Err(format!(
                "executing-head iteration mismatch: expected {}, observed {}",
                dispatch.iteration_id, self.iteration_id
            ));
        }
        if self.packet_digest != dispatch.packet_digest {
            return Err("executing-head packet digest mismatch".to_string());
        }
        if !is_sha256(&self.packet_digest) {
            return Err("executing-head packet digest is not lowercase SHA-256".to_string());
        }
        Ok(())
    }

    fn context<'a>(&'a self, dispatch: &'a ExecutionDispatchV1) -> MissionHeadContext<'a> {
        MissionHeadContext {
            brain_id: &dispatch.brain_id,
            mission_id: &dispatch.mission_id,
            head: Some(MissionHeadSnapshot {
                head_id: &self.head_id,
                state: self.state,
                iteration_id: self.iteration_id,
                packet_digest: &self.packet_digest,
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerDispatchEntryV1 {
    pub schema: String,
    pub dispatch: ExecutionDispatchV1,
    pub state: ExecutionDispatchState,
    pub revision: u64,
    pub registered_at: u64,
    pub updated_at: u64,
    pub ack: Option<ExecutionDispatchAckV1>,
    pub executing_head: Option<ExecutionMissionHeadV1>,
    pub result: Option<ExecutionResultV1>,
    pub result_transition_head_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerIntentRegistration {
    Registered,
    Deduplicated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchMutation {
    Applied,
    Deduplicated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnerReconciliationAction {
    RedeliverIntent {
        execution_id: String,
        dispatch: ExecutionDispatchV1,
    },
    ExpireIntent {
        execution_id: String,
        deadline_at: u64,
    },
    ApplyExecutingTransition {
        execution_id: String,
        ack: ExecutionDispatchAckV1,
    },
    AwaitResult {
        execution_id: String,
        executing_head: ExecutionMissionHeadV1,
    },
    ApplyResultTransition {
        execution_id: String,
        result: ExecutionResultV1,
        target_state: MissionState,
    },
    Settled {
        execution_id: String,
        state: ExecutionDispatchState,
        resulting_head_id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunnerInboxState {
    Claimed,
    Started,
    Acked,
    Completed,
    Failed,
}

impl RunnerInboxState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessClaimV1 {
    pub schema: String,
    pub claim_id: String,
    pub execution_id: String,
    pub dispatch_digest: String,
    pub runner_id: String,
    pub claimed_at: u64,
    pub claim_digest: String,
}

#[derive(Serialize)]
struct ProcessClaimCore<'a> {
    schema: &'a str,
    claim_id: &'a str,
    execution_id: &'a str,
    dispatch_digest: &'a str,
    runner_id: &'a str,
    claimed_at: u64,
}

impl ProcessClaimV1 {
    fn new(
        dispatch: &ExecutionDispatchV1,
        claimed_at: u64,
    ) -> Result<Self, ExecutionDispatchError> {
        let claim_id = digest_parts(
            "m1nd-process-claim-id-v1",
            &[
                &dispatch.execution_id,
                &dispatch.dispatch_digest,
                &dispatch.runner_id,
            ],
        );
        let mut claim = Self {
            schema: PROCESS_CLAIM_SCHEMA.to_string(),
            claim_id,
            execution_id: dispatch.execution_id.clone(),
            dispatch_digest: dispatch.dispatch_digest.clone(),
            runner_id: dispatch.runner_id.clone(),
            claimed_at,
            claim_digest: String::new(),
        };
        claim.claim_digest = claim.compute_digest()?;
        Ok(claim)
    }

    fn compute_digest(&self) -> Result<String, serde_json::Error> {
        let core = ProcessClaimCore {
            schema: &self.schema,
            claim_id: &self.claim_id,
            execution_id: &self.execution_id,
            dispatch_digest: &self.dispatch_digest,
            runner_id: &self.runner_id,
            claimed_at: self.claimed_at,
        };
        digest_json(PROCESS_CLAIM_DIGEST_DOMAIN, &core)
    }

    fn validate_against(&self, dispatch: &ExecutionDispatchV1) -> Result<(), String> {
        if self.schema != PROCESS_CLAIM_SCHEMA {
            return Err(format!(
                "unsupported process-claim schema '{}'",
                self.schema
            ));
        }
        validate_identifier("claim_id", &self.claim_id)?;
        if self.execution_id != dispatch.execution_id
            || self.dispatch_digest != dispatch.dispatch_digest
            || self.runner_id != dispatch.runner_id
        {
            return Err("process claim does not bind the exact dispatch".to_string());
        }
        if !is_sha256(&self.claim_digest) {
            return Err("claim digest is not lowercase SHA-256".to_string());
        }
        let expected = self.compute_digest().map_err(|error| error.to_string())?;
        if self.claim_digest != expected {
            return Err("process-claim digest mismatch".to_string());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerInboxEntryV1 {
    pub schema: String,
    pub dispatch: ExecutionDispatchV1,
    pub state: RunnerInboxState,
    pub revision: u64,
    pub claim: ProcessClaimV1,
    pub updated_at: u64,
    pub process_fingerprint: Option<String>,
    pub started_at: Option<u64>,
    pub ack: Option<ExecutionDispatchAckV1>,
    pub executing_head: Option<ExecutionMissionHeadV1>,
    pub result: Option<ExecutionResultV1>,
}

impl RunnerInboxEntryV1 {
    /// Revalidate a durable runner snapshot before it crosses into the owner
    /// service. This proves structural and exact dispatch bindings; signature
    /// bytes remain opaque at this layer.
    pub fn validate_for_service(&self) -> Result<(), ExecutionDispatchError> {
        validate_runner_shape(self).map_err(|detail| {
            ExecutionDispatchError::refused("invalid_runner_inbox_snapshot", detail)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnPermitV1 {
    pub claim: ProcessClaimV1,
    pub dispatch: ExecutionDispatchV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunnerClaimOutcome {
    Spawn(Box<SpawnPermitV1>),
    AlreadyClaimed {
        claim: ProcessClaimV1,
        state: RunnerInboxState,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunnerReconciliationAction {
    ClaimStalledNoRespawn {
        execution_id: String,
        claim: ProcessClaimV1,
    },
    AcceptanceAckRequired {
        execution_id: String,
        claim: ProcessClaimV1,
        process_fingerprint: String,
        started_at: u64,
    },
    AwaitExecutingTransition {
        execution_id: String,
        ack: ExecutionDispatchAckV1,
    },
    ObserveProcess {
        execution_id: String,
        executing_head: ExecutionMissionHeadV1,
        process_fingerprint: String,
    },
    DeliverResult {
        execution_id: String,
        result: ExecutionResultV1,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchFailpoint {
    OwnerIntent,
    RunnerClaim,
    RunnerStarted,
    OwnerAck,
    OwnerResult,
}

#[derive(Debug)]
pub enum ExecutionDispatchError {
    Refused { code: &'static str, detail: String },
    Corruption { offset: u64, detail: String },
    Io(io::Error),
    Json(serde_json::Error),
    Contract(MissionContractError),
    SimulatedCrash { point: DispatchFailpoint },
}

impl ExecutionDispatchError {
    fn refused(code: &'static str, detail: impl Into<String>) -> Self {
        Self::Refused {
            code,
            detail: detail.into(),
        }
    }

    fn corruption(offset: u64, detail: impl Into<String>) -> Self {
        Self::Corruption {
            offset,
            detail: detail.into(),
        }
    }

    pub const fn code(&self) -> &'static str {
        match self {
            Self::Refused { code, .. } => code,
            Self::Corruption { .. } => "dispatch_journal_corruption",
            Self::Io(_) => "dispatch_journal_io",
            Self::Json(_) => "dispatch_journal_json",
            Self::Contract(_) => "dispatch_contract_refused",
            Self::SimulatedCrash { .. } => "simulated_dispatch_crash",
        }
    }
}

impl fmt::Display for ExecutionDispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused { code, detail } => write!(formatter, "{code}: {detail}"),
            Self::Corruption { offset, detail } => {
                write!(
                    formatter,
                    "dispatch journal corruption at byte {offset}: {detail}"
                )
            }
            Self::Io(error) => write!(formatter, "dispatch journal I/O error: {error}"),
            Self::Json(error) => write!(formatter, "dispatch journal JSON error: {error}"),
            Self::Contract(error) => write!(formatter, "dispatch contract refused: {error}"),
            Self::SimulatedCrash { point } => {
                write!(formatter, "simulated crash after durable sync at {point:?}")
            }
        }
    }
}

impl Error for ExecutionDispatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Contract(error) => Some(error),
            Self::Refused { .. } | Self::Corruption { .. } | Self::SimulatedCrash { .. } => None,
        }
    }
}

impl From<io::Error> for ExecutionDispatchError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ExecutionDispatchError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<MissionContractError> for ExecutionDispatchError {
    fn from(error: MissionContractError) -> Self {
        Self::Contract(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JournalSurface {
    Owner,
    Runner,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "surface", content = "snapshot", rename_all = "snake_case")]
enum DispatchJournalEventV1 {
    Owner(OwnerDispatchEntryV1),
    Runner(RunnerInboxEntryV1),
}

impl DispatchJournalEventV1 {
    const fn surface(&self) -> JournalSurface {
        match self {
            Self::Owner(_) => JournalSurface::Owner,
            Self::Runner(_) => JournalSurface::Runner,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DispatchJournalRecordV1 {
    schema: String,
    sequence: u64,
    previous_record_digest: Option<String>,
    event: DispatchJournalEventV1,
    record_digest: String,
}

#[derive(Serialize)]
struct DispatchJournalRecordCore<'a> {
    schema: &'a str,
    sequence: u64,
    previous_record_digest: &'a Option<String>,
    event: &'a DispatchJournalEventV1,
}

impl DispatchJournalRecordV1 {
    fn new(
        sequence: u64,
        previous_record_digest: Option<String>,
        event: DispatchJournalEventV1,
    ) -> Result<Self, serde_json::Error> {
        let mut record = Self {
            schema: DISPATCH_JOURNAL_RECORD_SCHEMA.to_string(),
            sequence,
            previous_record_digest,
            event,
            record_digest: String::new(),
        };
        record.record_digest = record.compute_digest()?;
        Ok(record)
    }

    fn compute_digest(&self) -> Result<String, serde_json::Error> {
        digest_json(
            DISPATCH_JOURNAL_DIGEST_DOMAIN,
            &DispatchJournalRecordCore {
                schema: &self.schema,
                sequence: self.sequence,
                previous_record_digest: &self.previous_record_digest,
                event: &self.event,
            },
        )
    }
}

struct DispatchJournalWriter {
    file: File,
    next_sequence: u64,
    last_digest: Option<String>,
    poisoned: bool,
}

impl DispatchJournalWriter {
    fn open(
        path: &Path,
        expected_surface: JournalSurface,
    ) -> Result<(Self, Vec<(u64, DispatchJournalEventV1)>), ExecutionDispatchError> {
        let created = match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ExecutionDispatchError::refused(
                    "dispatch_journal_symlink_refused",
                    format!("journal path '{}' is a symlink", path.display()),
                ));
            }
            Ok(_) => false,
            Err(error) if error.kind() == io::ErrorKind::NotFound => true,
            Err(error) => return Err(error.into()),
        };

        let raw_parent = path.parent().ok_or_else(|| {
            ExecutionDispatchError::refused(
                "dispatch_journal_parent_missing",
                "journal path has no parent",
            )
        })?;
        let parent = if raw_parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            raw_parent
        };
        fs::create_dir_all(parent)?;

        let mut options = OpenOptions::new();
        options.read(true).append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(path)?;
        lock_exclusive(&file)?;
        if created {
            file.sync_all()?;
            sync_parent_directory(parent)?;
        }

        let mut reader = file.try_clone()?;
        reader.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        let (events, next_sequence, last_digest) =
            replay_dispatch_journal(&bytes, expected_surface)?;

        Ok((
            Self {
                file,
                next_sequence,
                last_digest,
                poisoned: false,
            },
            events,
        ))
    }

    fn append(&mut self, event: DispatchJournalEventV1) -> Result<(), ExecutionDispatchError> {
        if self.poisoned {
            return Err(ExecutionDispatchError::refused(
                "dispatch_journal_writer_poisoned",
                "a previous write had uncertain durability",
            ));
        }
        let next_sequence = self.next_sequence.checked_add(1).ok_or_else(|| {
            ExecutionDispatchError::refused(
                "dispatch_journal_sequence_exhausted",
                "journal sequence reached u64::MAX",
            )
        })?;
        let record =
            DispatchJournalRecordV1::new(self.next_sequence, self.last_digest.clone(), event)?;
        let mut bytes = serde_json::to_vec(&record)?;
        bytes.push(b'\n');
        if let Err(error) = self
            .file
            .write_all(&bytes)
            .and_then(|_| self.file.sync_all())
        {
            self.poisoned = true;
            return Err(error.into());
        }
        self.next_sequence = next_sequence;
        self.last_digest = Some(record.record_digest);
        Ok(())
    }

    fn simulate_crash(&mut self, point: DispatchFailpoint) -> ExecutionDispatchError {
        self.poisoned = true;
        ExecutionDispatchError::SimulatedCrash { point }
    }
}

impl Drop for DispatchJournalWriter {
    fn drop(&mut self) {
        unlock(&self.file);
    }
}

type ReplayEvents = (Vec<(u64, DispatchJournalEventV1)>, u64, Option<String>);

fn replay_dispatch_journal(
    bytes: &[u8],
    expected_surface: JournalSurface,
) -> Result<ReplayEvents, ExecutionDispatchError> {
    if bytes.is_empty() {
        return Ok((Vec::new(), 0, None));
    }
    if !bytes.ends_with(b"\n") {
        return Err(ExecutionDispatchError::corruption(
            bytes.len() as u64,
            "torn or unterminated journal tail; no record was guessed",
        ));
    }

    let mut events = Vec::new();
    let mut expected_sequence = 0_u64;
    let mut previous_digest: Option<String> = None;
    let mut offset = 0_u64;
    for line in bytes.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            offset = offset.saturating_add(1);
            continue;
        }
        let record: DispatchJournalRecordV1 = serde_json::from_slice(line).map_err(|error| {
            ExecutionDispatchError::corruption(offset, format!("invalid JSON record: {error}"))
        })?;
        if record.schema != DISPATCH_JOURNAL_RECORD_SCHEMA {
            return Err(ExecutionDispatchError::corruption(
                offset,
                format!("unsupported journal schema '{}'", record.schema),
            ));
        }
        if record.sequence != expected_sequence {
            return Err(ExecutionDispatchError::corruption(
                offset,
                format!(
                    "sequence mismatch: expected {expected_sequence}, observed {}",
                    record.sequence
                ),
            ));
        }
        if record.previous_record_digest != previous_digest {
            return Err(ExecutionDispatchError::corruption(
                offset,
                "previous-record digest mismatch",
            ));
        }
        if record.event.surface() != expected_surface {
            return Err(ExecutionDispatchError::corruption(
                offset,
                "journal contains a record for the wrong dispatch surface",
            ));
        }
        let computed = record.compute_digest()?;
        if record.record_digest != computed {
            return Err(ExecutionDispatchError::corruption(
                offset,
                "record digest mismatch",
            ));
        }
        previous_digest = Some(record.record_digest);
        events.push((offset, record.event));
        expected_sequence = expected_sequence.checked_add(1).ok_or_else(|| {
            ExecutionDispatchError::corruption(offset, "journal sequence exhausted")
        })?;
        offset = offset.saturating_add(line.len() as u64 + 1);
    }
    Ok((events, expected_sequence, previous_digest))
}

pub struct OwnerExecutionOutbox {
    journal: DispatchJournalWriter,
    entries: BTreeMap<String, OwnerDispatchEntryV1>,
}

impl OwnerExecutionOutbox {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ExecutionDispatchError> {
        let (journal, events) = DispatchJournalWriter::open(path.as_ref(), JournalSurface::Owner)?;
        let mut entries = BTreeMap::new();
        for (offset, event) in events {
            let DispatchJournalEventV1::Owner(entry) = event else {
                return Err(ExecutionDispatchError::corruption(
                    offset,
                    "runner snapshot reached owner replay",
                ));
            };
            apply_owner_replay(&mut entries, entry, offset)?;
        }
        Ok(Self { journal, entries })
    }

    pub fn get(&self, execution_id: &str) -> Option<&OwnerDispatchEntryV1> {
        self.entries.get(execution_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn register_intent(
        &mut self,
        dispatch: ExecutionDispatchV1,
        now_ms: u64,
    ) -> Result<OwnerIntentRegistration, ExecutionDispatchError> {
        self.register_intent_with_failpoint(dispatch, now_ms, None)
    }

    pub fn register_intent_with_failpoint(
        &mut self,
        dispatch: ExecutionDispatchV1,
        now_ms: u64,
        failpoint: Option<DispatchFailpoint>,
    ) -> Result<OwnerIntentRegistration, ExecutionDispatchError> {
        if self.preflight_intent(&dispatch, now_ms)? == OwnerIntentRegistration::Deduplicated {
            return Ok(OwnerIntentRegistration::Deduplicated);
        }
        let entry = OwnerDispatchEntryV1 {
            schema: OWNER_DISPATCH_ENTRY_SCHEMA.to_string(),
            dispatch: dispatch.clone(),
            state: ExecutionDispatchState::Intent,
            revision: 0,
            registered_at: now_ms,
            updated_at: now_ms,
            ack: None,
            executing_head: None,
            result: None,
            result_transition_head_id: None,
        };
        validate_owner_shape(&entry).map_err(|detail| {
            ExecutionDispatchError::refused("invalid_owner_dispatch_entry", detail)
        })?;
        self.journal
            .append(DispatchJournalEventV1::Owner(entry.clone()))?;
        if failpoint == Some(DispatchFailpoint::OwnerIntent) {
            return Err(self.journal.simulate_crash(DispatchFailpoint::OwnerIntent));
        }
        self.entries.insert(dispatch.execution_id, entry);
        Ok(OwnerIntentRegistration::Registered)
    }

    /// Validate identity, lifetime, and exact-dedup behavior without mutating
    /// the journal. MissionService uses this before publishing a DISPATCHING
    /// letter, then performs the durable registration immediately afterward.
    pub fn preflight_intent(
        &self,
        dispatch: &ExecutionDispatchV1,
        now_ms: u64,
    ) -> Result<OwnerIntentRegistration, ExecutionDispatchError> {
        if let Some(existing) = self.entries.get(&dispatch.execution_id) {
            if existing.dispatch == *dispatch {
                return Ok(OwnerIntentRegistration::Deduplicated);
            }
            return Err(identity_conflict("execution_id", &dispatch.execution_id));
        }
        refuse_identity_collisions_owner(&self.entries, dispatch)?;
        validate_new_dispatch(dispatch, now_ms)?;
        Ok(OwnerIntentRegistration::Registered)
    }

    pub fn record_ack(
        &mut self,
        ack: ExecutionDispatchAckV1,
        now_ms: u64,
    ) -> Result<DispatchMutation, ExecutionDispatchError> {
        self.record_ack_with_failpoint(ack, now_ms, None)
    }

    pub fn record_ack_with_failpoint(
        &mut self,
        ack: ExecutionDispatchAckV1,
        now_ms: u64,
        failpoint: Option<DispatchFailpoint>,
    ) -> Result<DispatchMutation, ExecutionDispatchError> {
        let current = self
            .entries
            .get(&ack.execution_id)
            .ok_or_else(|| unknown_execution(&ack.execution_id))?;
        if let Some(existing) = &current.ack {
            if existing == &ack {
                return Ok(DispatchMutation::Deduplicated);
            }
            return Err(ExecutionDispatchError::refused(
                "conflicting_execution_ack",
                "an execution may bind exactly one ACK",
            ));
        }
        if current.state != ExecutionDispatchState::Intent {
            return Err(illegal_owner_transition(current.state, "record ACK"));
        }
        ack.validate_against(&current.dispatch)?;
        if ack.accepted_at > now_ms {
            return Err(ExecutionDispatchError::refused(
                "ack_from_future",
                "ACK acceptance time is later than the observation time",
            ));
        }
        ensure_monotonic_time(current.updated_at, now_ms)?;
        let mut next = current.clone();
        next.state = ExecutionDispatchState::Acked;
        next.revision = next_revision(current.revision)?;
        next.updated_at = now_ms;
        next.ack = Some(ack);
        validate_owner_transition(current, &next).map_err(|detail| {
            ExecutionDispatchError::refused("invalid_owner_dispatch_transition", detail)
        })?;
        self.journal
            .append(DispatchJournalEventV1::Owner(next.clone()))?;
        if failpoint == Some(DispatchFailpoint::OwnerAck) {
            return Err(self.journal.simulate_crash(DispatchFailpoint::OwnerAck));
        }
        self.entries
            .insert(next.dispatch.execution_id.clone(), next);
        Ok(DispatchMutation::Applied)
    }

    pub fn mark_executing_transition(
        &mut self,
        execution_id: &str,
        ack_digest: &str,
        executing_head: ExecutionMissionHeadV1,
        now_ms: u64,
    ) -> Result<DispatchMutation, ExecutionDispatchError> {
        let current = self
            .entries
            .get(execution_id)
            .ok_or_else(|| unknown_execution(execution_id))?;
        let ack = current.ack.as_ref().ok_or_else(|| {
            ExecutionDispatchError::refused("missing_execution_ack", "entry has no durable ACK")
        })?;
        if ack.ack_digest != ack_digest {
            return Err(ExecutionDispatchError::refused(
                "ack_digest_mismatch",
                "EXECUTING transition does not bind the durable ACK",
            ));
        }
        if let Some(existing) = &current.executing_head {
            if existing == &executing_head {
                return Ok(DispatchMutation::Deduplicated);
            }
            return Err(ExecutionDispatchError::refused(
                "conflicting_executing_head",
                "execution is already bound to another EXECUTING head",
            ));
        }
        if current.state != ExecutionDispatchState::Acked {
            return Err(illegal_owner_transition(
                current.state,
                "record EXECUTING transition",
            ));
        }
        executing_head
            .validate_against(&current.dispatch)
            .map_err(|detail| ExecutionDispatchError::refused("invalid_executing_head", detail))?;
        ensure_monotonic_time(current.updated_at, now_ms)?;
        let mut next = current.clone();
        next.revision = next_revision(current.revision)?;
        next.updated_at = now_ms;
        next.executing_head = Some(executing_head);
        validate_owner_transition(current, &next).map_err(|detail| {
            ExecutionDispatchError::refused("invalid_owner_dispatch_transition", detail)
        })?;
        self.journal
            .append(DispatchJournalEventV1::Owner(next.clone()))?;
        self.entries.insert(execution_id.to_string(), next);
        Ok(DispatchMutation::Applied)
    }

    pub fn record_result(
        &mut self,
        result: ExecutionResultV1,
        now_ms: u64,
    ) -> Result<DispatchMutation, ExecutionDispatchError> {
        self.record_result_with_failpoint(result, now_ms, None)
    }

    pub fn record_result_with_failpoint(
        &mut self,
        result: ExecutionResultV1,
        now_ms: u64,
        failpoint: Option<DispatchFailpoint>,
    ) -> Result<DispatchMutation, ExecutionDispatchError> {
        let current = self
            .entries
            .get(&result.execution_id)
            .ok_or_else(|| unknown_execution(&result.execution_id))?;
        if let Some(existing) = &current.result {
            if existing == &result {
                return Ok(DispatchMutation::Deduplicated);
            }
            return Err(ExecutionDispatchError::refused(
                "conflicting_execution_result",
                "an execution may bind exactly one result",
            ));
        }
        if current.state != ExecutionDispatchState::Acked {
            return Err(illegal_owner_transition(current.state, "record result"));
        }
        let head = current.executing_head.as_ref().ok_or_else(|| {
            ExecutionDispatchError::refused(
                "executing_transition_not_applied",
                "result cannot precede the durable EXECUTING transition marker",
            )
        })?;
        result.validate_against(&current.dispatch, head.context(&current.dispatch))?;
        if result.ended_at > now_ms {
            return Err(ExecutionDispatchError::refused(
                "result_from_future",
                "result end time is later than the observation time",
            ));
        }
        ensure_monotonic_time(current.updated_at, now_ms)?;
        let mut next = current.clone();
        next.state = match result.outcome {
            ExecutionOutcome::Succeeded => ExecutionDispatchState::Completed,
            ExecutionOutcome::Failed => ExecutionDispatchState::Failed,
        };
        next.revision = next_revision(current.revision)?;
        next.updated_at = now_ms;
        next.result = Some(result);
        validate_owner_transition(current, &next).map_err(|detail| {
            ExecutionDispatchError::refused("invalid_owner_dispatch_transition", detail)
        })?;
        self.journal
            .append(DispatchJournalEventV1::Owner(next.clone()))?;
        if failpoint == Some(DispatchFailpoint::OwnerResult) {
            return Err(self.journal.simulate_crash(DispatchFailpoint::OwnerResult));
        }
        self.entries
            .insert(next.dispatch.execution_id.clone(), next);
        Ok(DispatchMutation::Applied)
    }

    pub fn mark_result_transition_applied(
        &mut self,
        execution_id: &str,
        result_digest: &str,
        resulting_head_id: impl Into<String>,
        now_ms: u64,
    ) -> Result<DispatchMutation, ExecutionDispatchError> {
        let resulting_head_id = resulting_head_id.into();
        validate_identifier("result_transition_head_id", &resulting_head_id).map_err(|detail| {
            ExecutionDispatchError::refused("invalid_result_transition_head", detail)
        })?;
        let current = self
            .entries
            .get(execution_id)
            .ok_or_else(|| unknown_execution(execution_id))?;
        let result = current.result.as_ref().ok_or_else(|| {
            ExecutionDispatchError::refused("missing_execution_result", "entry has no result")
        })?;
        if result.result_digest != result_digest {
            return Err(ExecutionDispatchError::refused(
                "result_digest_mismatch",
                "mission transition does not bind the durable execution result",
            ));
        }
        if let Some(existing) = &current.result_transition_head_id {
            if existing == &resulting_head_id {
                return Ok(DispatchMutation::Deduplicated);
            }
            return Err(ExecutionDispatchError::refused(
                "conflicting_result_transition",
                "result transition is already bound to another mission head",
            ));
        }
        if !current.state.is_terminal() {
            return Err(illegal_owner_transition(
                current.state,
                "record result transition",
            ));
        }
        ensure_monotonic_time(current.updated_at, now_ms)?;
        let mut next = current.clone();
        next.revision = next_revision(current.revision)?;
        next.updated_at = now_ms;
        next.result_transition_head_id = Some(resulting_head_id);
        validate_owner_transition(current, &next).map_err(|detail| {
            ExecutionDispatchError::refused("invalid_owner_dispatch_transition", detail)
        })?;
        self.journal
            .append(DispatchJournalEventV1::Owner(next.clone()))?;
        self.entries.insert(execution_id.to_string(), next);
        Ok(DispatchMutation::Applied)
    }

    pub fn reconcile(&self, now_ms: u64) -> Vec<OwnerReconciliationAction> {
        self.entries
            .values()
            .map(|entry| match entry.state {
                ExecutionDispatchState::Intent if now_ms >= entry.dispatch.deadline_at => {
                    OwnerReconciliationAction::ExpireIntent {
                        execution_id: entry.dispatch.execution_id.clone(),
                        deadline_at: entry.dispatch.deadline_at,
                    }
                }
                ExecutionDispatchState::Intent => OwnerReconciliationAction::RedeliverIntent {
                    execution_id: entry.dispatch.execution_id.clone(),
                    dispatch: entry.dispatch.clone(),
                },
                ExecutionDispatchState::Acked if entry.executing_head.is_none() => {
                    OwnerReconciliationAction::ApplyExecutingTransition {
                        execution_id: entry.dispatch.execution_id.clone(),
                        ack: entry
                            .ack
                            .clone()
                            .expect("validated ACKED owner entry has ACK"),
                    }
                }
                ExecutionDispatchState::Acked => OwnerReconciliationAction::AwaitResult {
                    execution_id: entry.dispatch.execution_id.clone(),
                    executing_head: entry
                        .executing_head
                        .clone()
                        .expect("validated owner entry has executing head"),
                },
                ExecutionDispatchState::Completed | ExecutionDispatchState::Failed
                    if entry.result_transition_head_id.is_none() =>
                {
                    let result = entry
                        .result
                        .clone()
                        .expect("validated terminal owner entry has result");
                    OwnerReconciliationAction::ApplyResultTransition {
                        execution_id: entry.dispatch.execution_id.clone(),
                        target_state: result.expected_transition(),
                        result,
                    }
                }
                ExecutionDispatchState::Completed | ExecutionDispatchState::Failed => {
                    OwnerReconciliationAction::Settled {
                        execution_id: entry.dispatch.execution_id.clone(),
                        state: entry.state,
                        resulting_head_id: entry
                            .result_transition_head_id
                            .clone()
                            .expect("validated settled owner entry has transition head"),
                    }
                }
            })
            .collect()
    }
}

pub struct RunnerExecutionInbox {
    runner_id: String,
    journal: DispatchJournalWriter,
    entries: BTreeMap<String, RunnerInboxEntryV1>,
}

impl RunnerExecutionInbox {
    pub fn open(
        path: impl AsRef<Path>,
        runner_id: impl Into<String>,
    ) -> Result<Self, ExecutionDispatchError> {
        let runner_id = runner_id.into();
        validate_identifier("runner_id", &runner_id)
            .map_err(|detail| ExecutionDispatchError::refused("invalid_runner_id", detail))?;
        let (journal, events) = DispatchJournalWriter::open(path.as_ref(), JournalSurface::Runner)?;
        let mut entries = BTreeMap::new();
        for (offset, event) in events {
            let DispatchJournalEventV1::Runner(entry) = event else {
                return Err(ExecutionDispatchError::corruption(
                    offset,
                    "owner snapshot reached runner replay",
                ));
            };
            if entry.dispatch.runner_id != runner_id {
                return Err(ExecutionDispatchError::corruption(
                    offset,
                    format!(
                        "journal runner mismatch: configured '{runner_id}', observed '{}'",
                        entry.dispatch.runner_id
                    ),
                ));
            }
            apply_runner_replay(&mut entries, entry, offset)?;
        }
        Ok(Self {
            runner_id,
            journal,
            entries,
        })
    }

    pub fn get(&self, execution_id: &str) -> Option<&RunnerInboxEntryV1> {
        self.entries.get(execution_id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn claim_for_spawn(
        &mut self,
        dispatch: ExecutionDispatchV1,
        now_ms: u64,
    ) -> Result<RunnerClaimOutcome, ExecutionDispatchError> {
        self.claim_for_spawn_with_failpoint(dispatch, now_ms, None)
    }

    pub fn claim_for_spawn_with_failpoint(
        &mut self,
        dispatch: ExecutionDispatchV1,
        now_ms: u64,
        failpoint: Option<DispatchFailpoint>,
    ) -> Result<RunnerClaimOutcome, ExecutionDispatchError> {
        if dispatch.runner_id != self.runner_id {
            return Err(ExecutionDispatchError::refused(
                "wrong_dispatch_runner",
                format!(
                    "configured runner '{}', dispatch runner '{}'",
                    self.runner_id, dispatch.runner_id
                ),
            ));
        }
        if let Some(existing) = self.entries.get(&dispatch.execution_id) {
            if existing.dispatch == dispatch {
                return Ok(RunnerClaimOutcome::AlreadyClaimed {
                    claim: existing.claim.clone(),
                    state: existing.state,
                });
            }
            return Err(identity_conflict("execution_id", &dispatch.execution_id));
        }
        refuse_identity_collisions_runner(&self.entries, &dispatch)?;
        validate_new_dispatch(&dispatch, now_ms)?;
        let claim = ProcessClaimV1::new(&dispatch, now_ms)?;
        let entry = RunnerInboxEntryV1 {
            schema: RUNNER_INBOX_ENTRY_SCHEMA.to_string(),
            dispatch: dispatch.clone(),
            state: RunnerInboxState::Claimed,
            revision: 0,
            claim: claim.clone(),
            updated_at: now_ms,
            process_fingerprint: None,
            started_at: None,
            ack: None,
            executing_head: None,
            result: None,
        };
        validate_runner_shape(&entry).map_err(|detail| {
            ExecutionDispatchError::refused("invalid_runner_inbox_entry", detail)
        })?;
        self.journal
            .append(DispatchJournalEventV1::Runner(entry.clone()))?;
        if failpoint == Some(DispatchFailpoint::RunnerClaim) {
            return Err(self.journal.simulate_crash(DispatchFailpoint::RunnerClaim));
        }
        self.entries.insert(dispatch.execution_id.clone(), entry);
        Ok(RunnerClaimOutcome::Spawn(Box::new(SpawnPermitV1 {
            claim,
            dispatch,
        })))
    }

    pub fn mark_process_started(
        &mut self,
        execution_id: &str,
        claim_id: &str,
        process_fingerprint: impl Into<String>,
        started_at: u64,
    ) -> Result<DispatchMutation, ExecutionDispatchError> {
        self.mark_process_started_with_failpoint(
            execution_id,
            claim_id,
            process_fingerprint,
            started_at,
            None,
        )
    }

    pub fn mark_process_started_with_failpoint(
        &mut self,
        execution_id: &str,
        claim_id: &str,
        process_fingerprint: impl Into<String>,
        started_at: u64,
        failpoint: Option<DispatchFailpoint>,
    ) -> Result<DispatchMutation, ExecutionDispatchError> {
        let process_fingerprint = process_fingerprint.into();
        validate_identifier("process_fingerprint", &process_fingerprint).map_err(|detail| {
            ExecutionDispatchError::refused("invalid_process_fingerprint", detail)
        })?;
        let current = self
            .entries
            .get(execution_id)
            .ok_or_else(|| unknown_execution(execution_id))?;
        if current.claim.claim_id != claim_id {
            return Err(ExecutionDispatchError::refused(
                "process_claim_mismatch",
                "process start does not bind the durable claim",
            ));
        }
        if let (Some(existing_fingerprint), Some(existing_started_at)) =
            (&current.process_fingerprint, current.started_at)
        {
            if existing_fingerprint == &process_fingerprint && existing_started_at == started_at {
                return Ok(DispatchMutation::Deduplicated);
            }
            return Err(ExecutionDispatchError::refused(
                "conflicting_process_start",
                "execution is already bound to another process start",
            ));
        }
        if current.state != RunnerInboxState::Claimed {
            return Err(illegal_runner_transition(
                current.state,
                "record process start",
            ));
        }
        if started_at < current.claim.claimed_at || started_at >= current.dispatch.deadline_at {
            return Err(ExecutionDispatchError::refused(
                "invalid_process_start_time",
                "process start must be at or after CLAIM and before dispatch deadline",
            ));
        }
        let mut next = current.clone();
        next.state = RunnerInboxState::Started;
        next.revision = next_revision(current.revision)?;
        next.updated_at = started_at;
        next.process_fingerprint = Some(process_fingerprint);
        next.started_at = Some(started_at);
        validate_runner_transition(current, &next).map_err(|detail| {
            ExecutionDispatchError::refused("invalid_runner_inbox_transition", detail)
        })?;
        self.journal
            .append(DispatchJournalEventV1::Runner(next.clone()))?;
        if failpoint == Some(DispatchFailpoint::RunnerStarted) {
            return Err(self
                .journal
                .simulate_crash(DispatchFailpoint::RunnerStarted));
        }
        self.entries.insert(execution_id.to_string(), next);
        Ok(DispatchMutation::Applied)
    }

    pub fn record_ack(
        &mut self,
        ack: ExecutionDispatchAckV1,
        now_ms: u64,
    ) -> Result<DispatchMutation, ExecutionDispatchError> {
        let current = self
            .entries
            .get(&ack.execution_id)
            .ok_or_else(|| unknown_execution(&ack.execution_id))?;
        if let Some(existing) = &current.ack {
            if existing == &ack {
                return Ok(DispatchMutation::Deduplicated);
            }
            return Err(ExecutionDispatchError::refused(
                "conflicting_execution_ack",
                "an execution may bind exactly one ACK",
            ));
        }
        if current.state != RunnerInboxState::Started {
            return Err(illegal_runner_transition(current.state, "record ACK"));
        }
        ack.validate_against(&current.dispatch)?;
        let started_at = current
            .started_at
            .expect("validated STARTED entry has time");
        if ack.accepted_at < started_at || ack.accepted_at > now_ms {
            return Err(ExecutionDispatchError::refused(
                "invalid_ack_observation_time",
                "ACK must be accepted after STARTED and no later than observation",
            ));
        }
        ensure_monotonic_time(current.updated_at, now_ms)?;
        let mut next = current.clone();
        next.state = RunnerInboxState::Acked;
        next.revision = next_revision(current.revision)?;
        next.updated_at = now_ms;
        next.ack = Some(ack);
        validate_runner_transition(current, &next).map_err(|detail| {
            ExecutionDispatchError::refused("invalid_runner_inbox_transition", detail)
        })?;
        self.journal
            .append(DispatchJournalEventV1::Runner(next.clone()))?;
        self.entries
            .insert(next.dispatch.execution_id.clone(), next);
        Ok(DispatchMutation::Applied)
    }

    pub fn observe_executing_transition(
        &mut self,
        execution_id: &str,
        ack_digest: &str,
        executing_head: ExecutionMissionHeadV1,
        now_ms: u64,
    ) -> Result<DispatchMutation, ExecutionDispatchError> {
        let current = self
            .entries
            .get(execution_id)
            .ok_or_else(|| unknown_execution(execution_id))?;
        let ack = current.ack.as_ref().ok_or_else(|| {
            ExecutionDispatchError::refused("missing_execution_ack", "entry has no durable ACK")
        })?;
        if ack.ack_digest != ack_digest {
            return Err(ExecutionDispatchError::refused(
                "ack_digest_mismatch",
                "EXECUTING observation does not bind the durable ACK",
            ));
        }
        if let Some(existing) = &current.executing_head {
            if existing == &executing_head {
                return Ok(DispatchMutation::Deduplicated);
            }
            return Err(ExecutionDispatchError::refused(
                "conflicting_executing_head",
                "runner already observed another EXECUTING head",
            ));
        }
        if current.state != RunnerInboxState::Acked {
            return Err(illegal_runner_transition(
                current.state,
                "observe EXECUTING transition",
            ));
        }
        executing_head
            .validate_against(&current.dispatch)
            .map_err(|detail| ExecutionDispatchError::refused("invalid_executing_head", detail))?;
        ensure_monotonic_time(current.updated_at, now_ms)?;
        let mut next = current.clone();
        next.revision = next_revision(current.revision)?;
        next.updated_at = now_ms;
        next.executing_head = Some(executing_head);
        validate_runner_transition(current, &next).map_err(|detail| {
            ExecutionDispatchError::refused("invalid_runner_inbox_transition", detail)
        })?;
        self.journal
            .append(DispatchJournalEventV1::Runner(next.clone()))?;
        self.entries.insert(execution_id.to_string(), next);
        Ok(DispatchMutation::Applied)
    }

    pub fn record_result(
        &mut self,
        result: ExecutionResultV1,
        now_ms: u64,
    ) -> Result<DispatchMutation, ExecutionDispatchError> {
        let current = self
            .entries
            .get(&result.execution_id)
            .ok_or_else(|| unknown_execution(&result.execution_id))?;
        if let Some(existing) = &current.result {
            if existing == &result {
                return Ok(DispatchMutation::Deduplicated);
            }
            return Err(ExecutionDispatchError::refused(
                "conflicting_execution_result",
                "an execution may bind exactly one result",
            ));
        }
        if current.state != RunnerInboxState::Acked {
            return Err(illegal_runner_transition(current.state, "record result"));
        }
        let head = current.executing_head.as_ref().ok_or_else(|| {
            ExecutionDispatchError::refused(
                "executing_transition_not_observed",
                "runner result cannot precede the EXECUTING head observation",
            )
        })?;
        result.validate_against(&current.dispatch, head.context(&current.dispatch))?;
        if Some(result.started_at) != current.started_at {
            return Err(ExecutionDispatchError::refused(
                "result_process_start_mismatch",
                "result start time does not bind the durable process start",
            ));
        }
        if result.ended_at > now_ms {
            return Err(ExecutionDispatchError::refused(
                "result_from_future",
                "result end time is later than the observation time",
            ));
        }
        ensure_monotonic_time(current.updated_at, now_ms)?;
        let mut next = current.clone();
        next.state = match result.outcome {
            ExecutionOutcome::Succeeded => RunnerInboxState::Completed,
            ExecutionOutcome::Failed => RunnerInboxState::Failed,
        };
        next.revision = next_revision(current.revision)?;
        next.updated_at = now_ms;
        next.result = Some(result);
        validate_runner_transition(current, &next).map_err(|detail| {
            ExecutionDispatchError::refused("invalid_runner_inbox_transition", detail)
        })?;
        self.journal
            .append(DispatchJournalEventV1::Runner(next.clone()))?;
        self.entries
            .insert(next.dispatch.execution_id.clone(), next);
        Ok(DispatchMutation::Applied)
    }

    pub fn reconcile(&self) -> Vec<RunnerReconciliationAction> {
        self.entries
            .values()
            .map(|entry| match entry.state {
                RunnerInboxState::Claimed => RunnerReconciliationAction::ClaimStalledNoRespawn {
                    execution_id: entry.dispatch.execution_id.clone(),
                    claim: entry.claim.clone(),
                },
                RunnerInboxState::Started => RunnerReconciliationAction::AcceptanceAckRequired {
                    execution_id: entry.dispatch.execution_id.clone(),
                    claim: entry.claim.clone(),
                    process_fingerprint: entry
                        .process_fingerprint
                        .clone()
                        .expect("validated STARTED entry has process fingerprint"),
                    started_at: entry.started_at.expect("validated STARTED entry has time"),
                },
                RunnerInboxState::Acked if entry.executing_head.is_none() => {
                    RunnerReconciliationAction::AwaitExecutingTransition {
                        execution_id: entry.dispatch.execution_id.clone(),
                        ack: entry
                            .ack
                            .clone()
                            .expect("validated ACKED runner entry has ACK"),
                    }
                }
                RunnerInboxState::Acked => RunnerReconciliationAction::ObserveProcess {
                    execution_id: entry.dispatch.execution_id.clone(),
                    executing_head: entry
                        .executing_head
                        .clone()
                        .expect("validated runner entry has executing head"),
                    process_fingerprint: entry
                        .process_fingerprint
                        .clone()
                        .expect("validated runner entry has process fingerprint"),
                },
                RunnerInboxState::Completed | RunnerInboxState::Failed => {
                    RunnerReconciliationAction::DeliverResult {
                        execution_id: entry.dispatch.execution_id.clone(),
                        result: entry
                            .result
                            .clone()
                            .expect("validated terminal runner entry has result"),
                    }
                }
            })
            .collect()
    }
}

fn apply_owner_replay(
    entries: &mut BTreeMap<String, OwnerDispatchEntryV1>,
    incoming: OwnerDispatchEntryV1,
    offset: u64,
) -> Result<(), ExecutionDispatchError> {
    validate_owner_shape(&incoming)
        .map_err(|detail| ExecutionDispatchError::corruption(offset, detail))?;
    match entries.get(&incoming.dispatch.execution_id) {
        None => {
            if incoming.revision != 0 || incoming.state != ExecutionDispatchState::Intent {
                return Err(ExecutionDispatchError::corruption(
                    offset,
                    "first owner snapshot must be revision 0 INTENT",
                ));
            }
            refuse_identity_collisions_owner(entries, &incoming.dispatch)
                .map_err(|error| ExecutionDispatchError::corruption(offset, error.to_string()))?;
        }
        Some(current) => validate_owner_transition(current, &incoming)
            .map_err(|detail| ExecutionDispatchError::corruption(offset, detail))?,
    }
    entries.insert(incoming.dispatch.execution_id.clone(), incoming);
    Ok(())
}

fn validate_owner_shape(entry: &OwnerDispatchEntryV1) -> Result<(), String> {
    if entry.schema != OWNER_DISPATCH_ENTRY_SCHEMA {
        return Err(format!("unsupported owner-entry schema '{}'", entry.schema));
    }
    validate_stored_dispatch(&entry.dispatch, entry.registered_at)?;
    if entry.updated_at < entry.registered_at {
        return Err("owner entry updated_at precedes registered_at".to_string());
    }
    if entry.revision == 0 && entry.updated_at != entry.registered_at {
        return Err("initial owner snapshot time differs from registration time".to_string());
    }
    if let Some(ack) = &entry.ack {
        ack.validate_against(&entry.dispatch)
            .map_err(|error| error.to_string())?;
    }
    if let Some(head) = &entry.executing_head {
        head.validate_against(&entry.dispatch)?;
    }
    if let Some(result) = &entry.result {
        let head = entry
            .executing_head
            .as_ref()
            .ok_or_else(|| "owner result exists without an EXECUTING head".to_string())?;
        result
            .validate_against(&entry.dispatch, head.context(&entry.dispatch))
            .map_err(|error| error.to_string())?;
    }
    if let Some(head_id) = &entry.result_transition_head_id {
        validate_identifier("result_transition_head_id", head_id)?;
    }

    match entry.state {
        ExecutionDispatchState::Intent => {
            if entry.ack.is_some()
                || entry.executing_head.is_some()
                || entry.result.is_some()
                || entry.result_transition_head_id.is_some()
            {
                return Err("INTENT owner entry contains later-phase evidence".to_string());
            }
        }
        ExecutionDispatchState::Acked => {
            if entry.ack.is_none()
                || entry.result.is_some()
                || entry.result_transition_head_id.is_some()
            {
                return Err("ACKED owner entry has inconsistent evidence".to_string());
            }
        }
        ExecutionDispatchState::Completed | ExecutionDispatchState::Failed => {
            if entry.ack.is_none() || entry.executing_head.is_none() || entry.result.is_none() {
                return Err("terminal owner entry lacks ACK, EXECUTING head, or result".to_string());
            }
            let result = entry.result.as_ref().expect("presence checked");
            let expected_state = match result.outcome {
                ExecutionOutcome::Succeeded => ExecutionDispatchState::Completed,
                ExecutionOutcome::Failed => ExecutionDispatchState::Failed,
            };
            if entry.state != expected_state {
                return Err("terminal owner state disagrees with execution outcome".to_string());
            }
        }
    }
    Ok(())
}

fn validate_owner_transition(
    current: &OwnerDispatchEntryV1,
    next: &OwnerDispatchEntryV1,
) -> Result<(), String> {
    validate_owner_shape(next)?;
    validate_snapshot_identity(
        &current.dispatch,
        &next.dispatch,
        current.registered_at,
        next.registered_at,
        current.revision,
        next.revision,
        current.updated_at,
        next.updated_at,
    )?;
    match (current.state, next.state) {
        (ExecutionDispatchState::Intent, ExecutionDispatchState::Acked) => {
            if current.ack.is_some()
                || next.ack.is_none()
                || current.executing_head != next.executing_head
                || current.result != next.result
                || current.result_transition_head_id != next.result_transition_head_id
            {
                return Err("illegal INTENT -> ACKED owner evidence delta".to_string());
            }
        }
        (ExecutionDispatchState::Acked, ExecutionDispatchState::Acked) => {
            if current.ack != next.ack
                || current.executing_head.is_some()
                || next.executing_head.is_none()
                || current.result != next.result
                || current.result_transition_head_id != next.result_transition_head_id
            {
                return Err("illegal ACKED -> ACKED owner evidence delta".to_string());
            }
        }
        (ExecutionDispatchState::Acked, ExecutionDispatchState::Completed)
        | (ExecutionDispatchState::Acked, ExecutionDispatchState::Failed) => {
            if current.ack != next.ack
                || current.executing_head != next.executing_head
                || current.result.is_some()
                || next.result.is_none()
                || current.result_transition_head_id != next.result_transition_head_id
            {
                return Err("illegal ACKED -> terminal owner evidence delta".to_string());
            }
        }
        (ExecutionDispatchState::Completed, ExecutionDispatchState::Completed)
        | (ExecutionDispatchState::Failed, ExecutionDispatchState::Failed) => {
            if current.ack != next.ack
                || current.executing_head != next.executing_head
                || current.result != next.result
                || current.result_transition_head_id.is_some()
                || next.result_transition_head_id.is_none()
            {
                return Err("illegal terminal owner settlement delta".to_string());
            }
        }
        (from, to) => {
            return Err(format!(
                "illegal owner dispatch transition {from:?} -> {to:?}"
            ));
        }
    }
    Ok(())
}

fn apply_runner_replay(
    entries: &mut BTreeMap<String, RunnerInboxEntryV1>,
    incoming: RunnerInboxEntryV1,
    offset: u64,
) -> Result<(), ExecutionDispatchError> {
    validate_runner_shape(&incoming)
        .map_err(|detail| ExecutionDispatchError::corruption(offset, detail))?;
    match entries.get(&incoming.dispatch.execution_id) {
        None => {
            if incoming.revision != 0 || incoming.state != RunnerInboxState::Claimed {
                return Err(ExecutionDispatchError::corruption(
                    offset,
                    "first runner snapshot must be revision 0 CLAIMED",
                ));
            }
            refuse_identity_collisions_runner(entries, &incoming.dispatch)
                .map_err(|error| ExecutionDispatchError::corruption(offset, error.to_string()))?;
        }
        Some(current) => validate_runner_transition(current, &incoming)
            .map_err(|detail| ExecutionDispatchError::corruption(offset, detail))?,
    }
    entries.insert(incoming.dispatch.execution_id.clone(), incoming);
    Ok(())
}

fn validate_runner_shape(entry: &RunnerInboxEntryV1) -> Result<(), String> {
    if entry.schema != RUNNER_INBOX_ENTRY_SCHEMA {
        return Err(format!(
            "unsupported runner-entry schema '{}'",
            entry.schema
        ));
    }
    validate_stored_dispatch(&entry.dispatch, entry.claim.claimed_at)?;
    entry.claim.validate_against(&entry.dispatch)?;
    if entry.updated_at < entry.claim.claimed_at {
        return Err("runner entry updated_at precedes CLAIM".to_string());
    }
    if entry.revision == 0 && entry.updated_at != entry.claim.claimed_at {
        return Err("initial runner snapshot time differs from CLAIM time".to_string());
    }
    if let Some(fingerprint) = &entry.process_fingerprint {
        validate_identifier("process_fingerprint", fingerprint)?;
    }
    if entry.process_fingerprint.is_some() != entry.started_at.is_some() {
        return Err("process fingerprint and started_at presence disagree".to_string());
    }
    if let Some(started_at) = entry.started_at {
        if started_at < entry.claim.claimed_at || started_at >= entry.dispatch.deadline_at {
            return Err("stored process start is outside CLAIM/deadline window".to_string());
        }
    }
    if let Some(ack) = &entry.ack {
        ack.validate_against(&entry.dispatch)
            .map_err(|error| error.to_string())?;
        let started_at = entry
            .started_at
            .ok_or_else(|| "runner ACK exists without process start".to_string())?;
        if ack.accepted_at < started_at {
            return Err("runner ACK predates process start".to_string());
        }
    }
    if let Some(head) = &entry.executing_head {
        head.validate_against(&entry.dispatch)?;
    }
    if let Some(result) = &entry.result {
        let head = entry
            .executing_head
            .as_ref()
            .ok_or_else(|| "runner result exists without an EXECUTING head".to_string())?;
        result
            .validate_against(&entry.dispatch, head.context(&entry.dispatch))
            .map_err(|error| error.to_string())?;
        if Some(result.started_at) != entry.started_at {
            return Err("stored result does not bind stored process start".to_string());
        }
    }

    match entry.state {
        RunnerInboxState::Claimed => {
            if entry.process_fingerprint.is_some()
                || entry.started_at.is_some()
                || entry.ack.is_some()
                || entry.executing_head.is_some()
                || entry.result.is_some()
            {
                return Err("CLAIMED runner entry contains later-phase evidence".to_string());
            }
        }
        RunnerInboxState::Started => {
            if entry.process_fingerprint.is_none()
                || entry.started_at.is_none()
                || entry.ack.is_some()
                || entry.executing_head.is_some()
                || entry.result.is_some()
            {
                return Err("STARTED runner entry has inconsistent evidence".to_string());
            }
        }
        RunnerInboxState::Acked => {
            if entry.process_fingerprint.is_none()
                || entry.started_at.is_none()
                || entry.ack.is_none()
                || entry.result.is_some()
            {
                return Err("ACKED runner entry has inconsistent evidence".to_string());
            }
        }
        RunnerInboxState::Completed | RunnerInboxState::Failed => {
            if entry.process_fingerprint.is_none()
                || entry.started_at.is_none()
                || entry.ack.is_none()
                || entry.executing_head.is_none()
                || entry.result.is_none()
            {
                return Err("terminal runner entry lacks required evidence".to_string());
            }
            let result = entry.result.as_ref().expect("presence checked");
            let expected_state = match result.outcome {
                ExecutionOutcome::Succeeded => RunnerInboxState::Completed,
                ExecutionOutcome::Failed => RunnerInboxState::Failed,
            };
            if entry.state != expected_state {
                return Err("terminal runner state disagrees with execution outcome".to_string());
            }
        }
    }
    Ok(())
}

fn validate_runner_transition(
    current: &RunnerInboxEntryV1,
    next: &RunnerInboxEntryV1,
) -> Result<(), String> {
    validate_runner_shape(next)?;
    validate_snapshot_identity(
        &current.dispatch,
        &next.dispatch,
        current.claim.claimed_at,
        next.claim.claimed_at,
        current.revision,
        next.revision,
        current.updated_at,
        next.updated_at,
    )?;
    if current.claim != next.claim {
        return Err("durable process claim changed across snapshots".to_string());
    }
    match (current.state, next.state) {
        (RunnerInboxState::Claimed, RunnerInboxState::Started) => {
            if current.process_fingerprint.is_some()
                || next.process_fingerprint.is_none()
                || current.started_at.is_some()
                || next.started_at.is_none()
                || current.ack != next.ack
                || current.executing_head != next.executing_head
                || current.result != next.result
            {
                return Err("illegal CLAIMED -> STARTED runner evidence delta".to_string());
            }
        }
        (RunnerInboxState::Started, RunnerInboxState::Acked) => {
            if current.process_fingerprint != next.process_fingerprint
                || current.started_at != next.started_at
                || current.ack.is_some()
                || next.ack.is_none()
                || current.executing_head != next.executing_head
                || current.result != next.result
            {
                return Err("illegal STARTED -> ACKED runner evidence delta".to_string());
            }
        }
        (RunnerInboxState::Acked, RunnerInboxState::Acked) => {
            if current.process_fingerprint != next.process_fingerprint
                || current.started_at != next.started_at
                || current.ack != next.ack
                || current.executing_head.is_some()
                || next.executing_head.is_none()
                || current.result != next.result
            {
                return Err("illegal ACKED -> ACKED runner evidence delta".to_string());
            }
        }
        (RunnerInboxState::Acked, RunnerInboxState::Completed)
        | (RunnerInboxState::Acked, RunnerInboxState::Failed) => {
            if current.process_fingerprint != next.process_fingerprint
                || current.started_at != next.started_at
                || current.ack != next.ack
                || current.executing_head != next.executing_head
                || current.result.is_some()
                || next.result.is_none()
            {
                return Err("illegal ACKED -> terminal runner evidence delta".to_string());
            }
        }
        (from, to) => {
            return Err(format!(
                "illegal runner inbox transition {from:?} -> {to:?}"
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_snapshot_identity(
    current_dispatch: &ExecutionDispatchV1,
    next_dispatch: &ExecutionDispatchV1,
    current_registered_at: u64,
    next_registered_at: u64,
    current_revision: u64,
    next_revision_value: u64,
    current_updated_at: u64,
    next_updated_at: u64,
) -> Result<(), String> {
    if current_dispatch != next_dispatch {
        return Err("immutable signed dispatch changed across snapshots".to_string());
    }
    if current_registered_at != next_registered_at {
        return Err("registration/claim time changed across snapshots".to_string());
    }
    let expected_revision = current_revision
        .checked_add(1)
        .ok_or_else(|| "snapshot revision exhausted".to_string())?;
    if next_revision_value != expected_revision {
        return Err(format!(
            "snapshot revision mismatch: expected {expected_revision}, observed {next_revision_value}"
        ));
    }
    if next_updated_at < current_updated_at {
        return Err("snapshot time moved backwards".to_string());
    }
    Ok(())
}

fn validate_new_dispatch(
    dispatch: &ExecutionDispatchV1,
    now_ms: u64,
) -> Result<(), ExecutionDispatchError> {
    if dispatch.state != ExecutionDispatchState::Intent {
        return Err(ExecutionDispatchError::refused(
            "dispatch_not_intent",
            "new dispatch envelopes must be signed in INTENT state",
        ));
    }
    dispatch.validate(now_ms)?;
    Ok(())
}

fn validate_stored_dispatch(
    dispatch: &ExecutionDispatchV1,
    registered_at: u64,
) -> Result<(), String> {
    if dispatch.state != ExecutionDispatchState::Intent {
        return Err("stored signed dispatch is not immutable INTENT".to_string());
    }
    dispatch
        .validate(registered_at)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn refuse_identity_collisions_owner(
    entries: &BTreeMap<String, OwnerDispatchEntryV1>,
    dispatch: &ExecutionDispatchV1,
) -> Result<(), ExecutionDispatchError> {
    for existing in entries.values() {
        refuse_identity_collision(&existing.dispatch, dispatch)?;
    }
    Ok(())
}

fn refuse_identity_collisions_runner(
    entries: &BTreeMap<String, RunnerInboxEntryV1>,
    dispatch: &ExecutionDispatchV1,
) -> Result<(), ExecutionDispatchError> {
    for existing in entries.values() {
        refuse_identity_collision(&existing.dispatch, dispatch)?;
    }
    Ok(())
}

fn refuse_identity_collision(
    existing: &ExecutionDispatchV1,
    incoming: &ExecutionDispatchV1,
) -> Result<(), ExecutionDispatchError> {
    if existing.execution_id == incoming.execution_id {
        return Err(identity_conflict("execution_id", &incoming.execution_id));
    }
    if existing.idempotency_key == incoming.idempotency_key {
        return Err(identity_conflict(
            "idempotency_key",
            &incoming.idempotency_key,
        ));
    }
    if same_packet_binding(existing, incoming) {
        return Err(identity_conflict(
            "packet_runner_binding",
            &incoming.packet_digest,
        ));
    }
    Ok(())
}

fn same_packet_binding(left: &ExecutionDispatchV1, right: &ExecutionDispatchV1) -> bool {
    left.brain_id == right.brain_id
        && left.mission_id == right.mission_id
        && left.mission_head_id == right.mission_head_id
        && left.iteration_id == right.iteration_id
        && left.packet_digest == right.packet_digest
        && left.runner_id == right.runner_id
}

fn identity_conflict(field: &'static str, value: &str) -> ExecutionDispatchError {
    ExecutionDispatchError::refused(
        "dispatch_identity_conflict",
        format!("{field} '{value}' is already bound to a different dispatch"),
    )
}

fn unknown_execution(execution_id: &str) -> ExecutionDispatchError {
    ExecutionDispatchError::refused(
        "unknown_execution",
        format!("execution '{execution_id}' is not durably registered"),
    )
}

fn illegal_owner_transition(
    state: ExecutionDispatchState,
    operation: &'static str,
) -> ExecutionDispatchError {
    ExecutionDispatchError::refused(
        "illegal_owner_dispatch_transition",
        format!("cannot {operation} while owner state is {state:?}"),
    )
}

fn illegal_runner_transition(
    state: RunnerInboxState,
    operation: &'static str,
) -> ExecutionDispatchError {
    ExecutionDispatchError::refused(
        "illegal_runner_inbox_transition",
        format!("cannot {operation} while runner state is {state:?}"),
    )
}

fn ensure_monotonic_time(previous: u64, next: u64) -> Result<(), ExecutionDispatchError> {
    if next < previous {
        return Err(ExecutionDispatchError::refused(
            "dispatch_clock_regression",
            format!("observation time {next} precedes durable time {previous}"),
        ));
    }
    Ok(())
}

fn next_revision(current: u64) -> Result<u64, ExecutionDispatchError> {
    current.checked_add(1).ok_or_else(|| {
        ExecutionDispatchError::refused(
            "dispatch_revision_exhausted",
            "dispatch snapshot revision reached u64::MAX",
        )
    })
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(format!(
            "{field} must contain 1..=512 non-control characters"
        ));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn digest_parts(domain: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    for part in parts {
        hasher.update([0]);
        hasher.update(part.as_bytes());
    }
    hex_lower(&hasher.finalize())
}

fn digest_json<T: Serialize + ?Sized>(
    domain: &str,
    value: &T,
) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(value)?;
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    Ok(hex_lower(&hasher.finalize()))
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

#[cfg(unix)]
fn lock_exclusive(file: &File) -> Result<(), ExecutionDispatchError> {
    use std::os::fd::AsRawFd;
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        Ok(())
    } else {
        Err(ExecutionDispatchError::refused(
            "dispatch_journal_writer_lock_refused",
            io::Error::last_os_error().to_string(),
        ))
    }
}

#[cfg(not(unix))]
fn lock_exclusive(_file: &File) -> Result<(), ExecutionDispatchError> {
    Ok(())
}

#[cfg(unix)]
fn unlock(file: &File) {
    use std::os::fd::AsRawFd;
    let _ = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
}

#[cfg(not(unix))]
fn unlock(_file: &File) {}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> Result<(), ExecutionDispatchError> {
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> Result<(), ExecutionDispatchError> {
    Ok(())
}
