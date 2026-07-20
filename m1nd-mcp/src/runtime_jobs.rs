//! Durable, transport-independent supervision for potentially blocking work.
//!
//! A submitted operation is durably registered before its preparation closure
//! starts. Preparation may run beyond the caller deadline, but it never owns a
//! commit surface: only the registry-controlled commit closure can publish the
//! prepared value, and that closure is skipped after cancellation or timeout.
//!
//! The project-brain runtime binds this registry to a bounded per-brain
//! actor/OCC commit boundary. Transport adoption is still explicit: an ingress
//! is supervised only when its handler submits through that runtime API.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use m1nd_control::{ActionId, AuthorityVariant, Effect, Ingress};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const RUNTIME_JOB_SCHEMA: &str = "m1nd-runtime-job-v1";
pub const RUNTIME_JOB_BINDING_SCHEMA: &str = "m1nd-runtime-job-binding-v1";
pub const RUNTIME_JOB_AUTHORITY_SCHEMA: &str = "m1nd-runtime-job-authority-binding-v1";
pub const RUNTIME_JOB_TERMINAL_RESULT_SCHEMA: &str = "m1nd-runtime-job-terminal-result-v1";
pub const RUNTIME_JOB_JOURNAL_RECORD_SCHEMA: &str = "m1nd-runtime-job-journal-record-v1";
pub const RUNTIME_JOB_HEALTH_SCHEMA: &str = "m1nd-runtime-job-health-v1";
pub const DEFAULT_MAX_IN_FLIGHT_JOBS: usize = 64;
const RUNTIME_JOB_RECORD_DIGEST_DOMAIN: &str = "m1nd-runtime-job-journal-record-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeJobState {
    Pending,
    Running,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
}

impl RuntimeJobState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeJobAuthorityBindingV1 {
    pub schema: String,
    pub decision_id: String,
    pub authority_variant: AuthorityVariant,
    pub authority_epoch: u64,
    pub autonomy_epoch: u64,
    pub capability_id: Option<String>,
    pub authorization_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeJobBindingV1 {
    pub schema: String,
    pub organism_id: String,
    pub brain_id: String,
    pub mission_id: String,
    pub agent_id: String,
    pub action: ActionId,
    pub ingress: Ingress,
    pub effects: BTreeSet<Effect>,
    pub authority: RuntimeJobAuthorityBindingV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeJobRequestV1 {
    pub job_id: String,
    pub idempotency_key: String,
    pub binding: RuntimeJobBindingV1,
    pub snapshot_revision: u64,
    pub deadline_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeJobTerminalResultV1 {
    pub schema: String,
    pub code: String,
    pub message: String,
    pub output_digest: Option<String>,
    pub finished_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeJobV1 {
    pub schema: String,
    pub job_id: String,
    pub idempotency_key: String,
    pub binding: RuntimeJobBindingV1,
    pub snapshot_revision: u64,
    pub deadline_unix_ms: u64,
    pub state: RuntimeJobState,
    pub revision: u64,
    pub registered_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
    pub running_after_timeout: bool,
    /// Durable linearization point: cancellation may no longer claim the job
    /// was cancelled once the registry has reserved its commit.
    pub commit_in_progress: bool,
    pub state_reason: Option<String>,
    pub terminal_result: Option<RuntimeJobTerminalResultV1>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeJobSuccess {
    pub code: String,
    pub message: String,
    pub output_digest: Option<String>,
}

impl RuntimeJobSuccess {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            output_digest: None,
        }
    }

    pub fn with_output_digest(mut self, digest: impl Into<String>) -> Self {
        self.output_digest = Some(digest.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeJobFailure {
    pub code: String,
    pub message: String,
}

impl RuntimeJobFailure {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[derive(Clone, Debug)]
pub struct RuntimeJobContext {
    pub job_id: String,
    pub binding: RuntimeJobBindingV1,
    pub snapshot_revision: u64,
    pub deadline_unix_ms: u64,
    cancellation: CancellationToken,
}

impl RuntimeJobContext {
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub fn checkpoint(&self) -> Result<(), RuntimeJobFailure> {
        if self.is_cancelled() {
            return Err(RuntimeJobFailure::new(
                "operation_cancelled",
                "runtime job cancellation was requested",
            ));
        }
        if now_unix_ms().unwrap_or(u64::MAX) >= self.deadline_unix_ms {
            return Err(RuntimeJobFailure::new(
                "deadline_reached",
                "runtime job deadline was reached",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeJobWait {
    Terminal(RuntimeJobV1),
    ObservableNonTerminal(RuntimeJobV1),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShutdownIncomplete {
    pub active_job_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeJobHealthV1 {
    pub schema: String,
    pub accepting: bool,
    pub poisoned: bool,
    pub max_in_flight: usize,
    pub active_jobs: usize,
    pub total_jobs: usize,
    pub running_after_timeout: usize,
    pub commit_in_progress: usize,
    pub state_counts: BTreeMap<String, usize>,
}

#[derive(Debug)]
pub enum RuntimeJobError {
    Io(io::Error),
    Json(serde_json::Error),
    Refused {
        code: &'static str,
        detail: String,
    },
    Corruption {
        offset: u64,
        detail: String,
    },
    DuplicateJobId(String),
    DuplicateIdempotencyKey(String),
    UnknownJob(String),
    IllegalTransition {
        job_id: String,
        from: RuntimeJobState,
        to: RuntimeJobState,
    },
    RegistryShuttingDown,
    Overloaded {
        limit: usize,
        active: usize,
    },
    CancellationTooLate(String),
    RegistryPoisoned,
    Spawn(io::Error),
    ShutdownIncomplete(ShutdownIncomplete),
}

impl RuntimeJobError {
    fn refused(code: &'static str, detail: impl Into<String>) -> Self {
        Self::Refused {
            code,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for RuntimeJobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "runtime job I/O error: {error}"),
            Self::Json(error) => write!(formatter, "runtime job JSON error: {error}"),
            Self::Refused { code, detail } => write!(formatter, "{code}: {detail}"),
            Self::Corruption { offset, detail } => {
                write!(
                    formatter,
                    "runtime job journal corruption at byte {offset}: {detail}"
                )
            }
            Self::DuplicateJobId(job_id) => write!(formatter, "duplicate job id '{job_id}'"),
            Self::DuplicateIdempotencyKey(key) => {
                write!(formatter, "duplicate runtime job idempotency key '{key}'")
            }
            Self::UnknownJob(job_id) => write!(formatter, "unknown runtime job '{job_id}'"),
            Self::IllegalTransition { job_id, from, to } => {
                write!(
                    formatter,
                    "illegal runtime job transition for {job_id}: {from:?} -> {to:?}"
                )
            }
            Self::RegistryShuttingDown => {
                formatter.write_str("runtime job registry is shutting down")
            }
            Self::Overloaded { limit, active } => write!(
                formatter,
                "runtime job registry overloaded: {active} active jobs at limit {limit}"
            ),
            Self::CancellationTooLate(job_id) => write!(
                formatter,
                "runtime job '{job_id}' has crossed its durable commit reservation"
            ),
            Self::RegistryPoisoned => formatter.write_str(
                "runtime job registry is poisoned after an uncertain journal write or lock panic",
            ),
            Self::Spawn(error) => write!(formatter, "runtime job worker spawn failed: {error}"),
            Self::ShutdownIncomplete(report) => write!(
                formatter,
                "runtime job shutdown deadline elapsed with active jobs: {}",
                report.active_job_ids.join(", ")
            ),
        }
    }
}

impl Error for RuntimeJobError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) | Self::Spawn(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Refused { .. }
            | Self::Corruption { .. }
            | Self::DuplicateJobId(_)
            | Self::DuplicateIdempotencyKey(_)
            | Self::UnknownJob(_)
            | Self::IllegalTransition { .. }
            | Self::RegistryShuttingDown
            | Self::Overloaded { .. }
            | Self::CancellationTooLate(_)
            | Self::RegistryPoisoned
            | Self::ShutdownIncomplete(_) => None,
        }
    }
}

impl From<io::Error> for RuntimeJobError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for RuntimeJobError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeJobJournalRecordV1 {
    schema: String,
    sequence: u64,
    previous_record_digest: Option<String>,
    job: RuntimeJobV1,
    record_digest: String,
}

#[derive(Serialize)]
struct RuntimeJobJournalRecordCore<'a> {
    schema: &'a str,
    sequence: u64,
    previous_record_digest: &'a Option<String>,
    job: &'a RuntimeJobV1,
}

impl RuntimeJobJournalRecordV1 {
    fn new(sequence: u64, previous_record_digest: Option<String>, job: RuntimeJobV1) -> Self {
        let mut record = Self {
            schema: RUNTIME_JOB_JOURNAL_RECORD_SCHEMA.to_string(),
            sequence,
            previous_record_digest,
            job,
            record_digest: String::new(),
        };
        record.record_digest = record
            .compute_digest()
            .expect("typed journal record serializes");
        record
    }

    fn compute_digest(&self) -> Result<String, serde_json::Error> {
        let core = RuntimeJobJournalRecordCore {
            schema: &self.schema,
            sequence: self.sequence,
            previous_record_digest: &self.previous_record_digest,
            job: &self.job,
        };
        let bytes = serde_json::to_vec(&core)?;
        let mut hasher = Sha256::new();
        hasher.update(RUNTIME_JOB_RECORD_DIGEST_DOMAIN.as_bytes());
        hasher.update([0]);
        hasher.update(bytes);
        Ok(hex_lower(&hasher.finalize()))
    }
}

struct JournalWriter {
    path: PathBuf,
    file: File,
    next_sequence: u64,
    last_digest: Option<String>,
}

type JournalReplay = (BTreeMap<String, RuntimeJobV1>, u64, Option<String>);

impl JournalWriter {
    fn open(path: &Path) -> Result<(Self, BTreeMap<String, RuntimeJobV1>), RuntimeJobError> {
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if metadata.file_type().is_symlink() {
                return Err(RuntimeJobError::refused(
                    "journal_symlink_refused",
                    format!("journal path '{}' is a symlink", path.display()),
                ));
            }
        }

        let parent = path.parent().ok_or_else(|| {
            RuntimeJobError::refused("journal_parent_missing", "journal path has no parent")
        })?;
        fs::create_dir_all(parent)?;
        let created = !path.exists();

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
        let (jobs, next_sequence, last_digest) = replay_journal(&bytes)?;

        Ok((
            Self {
                path: path.to_path_buf(),
                file,
                next_sequence,
                last_digest,
            },
            jobs,
        ))
    }

    fn append(&mut self, job: RuntimeJobV1) -> Result<(), RuntimeJobError> {
        let record =
            RuntimeJobJournalRecordV1::new(self.next_sequence, self.last_digest.clone(), job);
        let mut bytes = serde_json::to_vec(&record)?;
        bytes.push(b'\n');
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.last_digest = Some(record.record_digest);
        Ok(())
    }
}

impl Drop for JournalWriter {
    fn drop(&mut self) {
        unlock(&self.file);
    }
}

fn replay_journal(bytes: &[u8]) -> Result<JournalReplay, RuntimeJobError> {
    if bytes.is_empty() {
        return Ok((BTreeMap::new(), 0, None));
    }
    if !bytes.ends_with(b"\n") {
        return Err(RuntimeJobError::Corruption {
            offset: bytes.len() as u64,
            detail: "torn or unterminated journal tail; no record was guessed".to_string(),
        });
    }

    let mut jobs = BTreeMap::new();
    let mut expected_sequence = 0_u64;
    let mut previous_digest: Option<String> = None;
    let mut offset = 0_u64;

    for line in bytes.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            offset = offset.saturating_add(1);
            continue;
        }
        let record: RuntimeJobJournalRecordV1 =
            serde_json::from_slice(line).map_err(|error| RuntimeJobError::Corruption {
                offset,
                detail: format!("invalid JSON record: {error}"),
            })?;
        if record.schema != RUNTIME_JOB_JOURNAL_RECORD_SCHEMA {
            return Err(RuntimeJobError::Corruption {
                offset,
                detail: format!("unsupported record schema '{}'", record.schema),
            });
        }
        if record.sequence != expected_sequence {
            return Err(RuntimeJobError::Corruption {
                offset,
                detail: format!(
                    "sequence mismatch: expected {expected_sequence}, observed {}",
                    record.sequence
                ),
            });
        }
        if record.previous_record_digest != previous_digest {
            return Err(RuntimeJobError::Corruption {
                offset,
                detail: "previous-record digest mismatch".to_string(),
            });
        }
        let computed = record.compute_digest()?;
        if record.record_digest != computed {
            return Err(RuntimeJobError::Corruption {
                offset,
                detail: "record digest mismatch".to_string(),
            });
        }
        validate_job_shape(&record.job)?;
        apply_replayed_job(&mut jobs, &record.job, offset)?;

        previous_digest = Some(record.record_digest);
        expected_sequence = expected_sequence.saturating_add(1);
        offset = offset.saturating_add(line.len() as u64 + 1);
    }

    Ok((jobs, expected_sequence, previous_digest))
}

fn apply_replayed_job(
    jobs: &mut BTreeMap<String, RuntimeJobV1>,
    incoming: &RuntimeJobV1,
    offset: u64,
) -> Result<(), RuntimeJobError> {
    match jobs.get(&incoming.job_id) {
        None => {
            if incoming.revision != 0
                || incoming.state != RuntimeJobState::Pending
                || incoming.commit_in_progress
            {
                return Err(RuntimeJobError::Corruption {
                    offset,
                    detail: format!(
                        "first record for '{}' is not revision 0 PENDING",
                        incoming.job_id
                    ),
                });
            }
        }
        Some(previous) => {
            if incoming.revision != previous.revision.saturating_add(1) {
                return Err(RuntimeJobError::Corruption {
                    offset,
                    detail: format!("non-contiguous revision for '{}'", incoming.job_id),
                });
            }
            if !same_job_identity(previous, incoming) {
                return Err(RuntimeJobError::Corruption {
                    offset,
                    detail: format!("job identity changed for '{}'", incoming.job_id),
                });
            }
            if !valid_snapshot_transition(previous, incoming) {
                return Err(RuntimeJobError::Corruption {
                    offset,
                    detail: format!(
                        "illegal replayed transition for '{}': {:?} -> {:?}",
                        incoming.job_id, previous.state, incoming.state
                    ),
                });
            }
            if previous.running_after_timeout && !incoming.running_after_timeout {
                return Err(RuntimeJobError::Corruption {
                    offset,
                    detail: format!("running_after_timeout regressed for '{}'", incoming.job_id),
                });
            }
        }
    }
    jobs.insert(incoming.job_id.clone(), incoming.clone());
    Ok(())
}

fn same_job_identity(left: &RuntimeJobV1, right: &RuntimeJobV1) -> bool {
    left.schema == right.schema
        && left.job_id == right.job_id
        && left.idempotency_key == right.idempotency_key
        && left.binding == right.binding
        && left.snapshot_revision == right.snapshot_revision
        && left.deadline_unix_ms == right.deadline_unix_ms
        && left.registered_at_unix_ms == right.registered_at_unix_ms
}

fn valid_transition(from: RuntimeJobState, to: RuntimeJobState) -> bool {
    matches!(
        (from, to),
        (RuntimeJobState::Pending, RuntimeJobState::Running)
            | (RuntimeJobState::Pending, RuntimeJobState::Cancelling)
            | (RuntimeJobState::Pending, RuntimeJobState::Cancelled)
            | (RuntimeJobState::Pending, RuntimeJobState::Failed)
            | (RuntimeJobState::Running, RuntimeJobState::Cancelling)
            | (RuntimeJobState::Running, RuntimeJobState::Succeeded)
            | (RuntimeJobState::Running, RuntimeJobState::Failed)
            | (RuntimeJobState::Cancelling, RuntimeJobState::Cancelled)
            | (RuntimeJobState::Cancelling, RuntimeJobState::Failed)
    )
}

fn valid_snapshot_transition(previous: &RuntimeJobV1, incoming: &RuntimeJobV1) -> bool {
    if incoming.commit_in_progress && incoming.state != RuntimeJobState::Running {
        return false;
    }
    if previous.commit_in_progress {
        return previous.state == RuntimeJobState::Running
            && matches!(
                incoming.state,
                RuntimeJobState::Succeeded | RuntimeJobState::Failed
            )
            && !incoming.commit_in_progress;
    }
    if incoming.commit_in_progress {
        return previous.state == RuntimeJobState::Running
            && incoming.state == RuntimeJobState::Running;
    }
    valid_transition(previous.state, incoming.state)
        || (previous.state == RuntimeJobState::Cancelling
            && incoming.state == RuntimeJobState::Cancelling
            && !previous.running_after_timeout
            && incoming.running_after_timeout)
}

struct JobEntry {
    job: RuntimeJobV1,
    cancellation: CancellationToken,
}

struct RegistryState {
    accepting: bool,
    poisoned: bool,
    max_in_flight: usize,
    jobs: BTreeMap<String, JobEntry>,
    idempotency: HashMap<String, String>,
    journal: JournalWriter,
}

struct RegistryCore {
    state: Mutex<RegistryState>,
    changed: Condvar,
}

impl RegistryCore {
    fn lock(&self) -> Result<MutexGuard<'_, RegistryState>, RuntimeJobError> {
        self.state
            .lock()
            .map_err(|_| RuntimeJobError::RegistryPoisoned)
    }

    fn append_then_replace(
        &self,
        state: &mut RegistryState,
        next: RuntimeJobV1,
    ) -> Result<(), RuntimeJobError> {
        if state.poisoned {
            return Err(RuntimeJobError::RegistryPoisoned);
        }
        if let Err(error) = state.journal.append(next.clone()) {
            state.poisoned = true;
            return Err(error);
        }
        let entry = state
            .jobs
            .get_mut(&next.job_id)
            .expect("transition target is registered");
        entry.job = next;
        self.changed.notify_all();
        Ok(())
    }

    fn transition(
        &self,
        job_id: &str,
        to: RuntimeJobState,
        reason: impl Into<String>,
        running_after_timeout: bool,
        terminal_result: Option<RuntimeJobTerminalResultV1>,
    ) -> Result<RuntimeJobV1, RuntimeJobError> {
        let mut state = self.lock()?;
        let current = state
            .jobs
            .get(job_id)
            .ok_or_else(|| RuntimeJobError::UnknownJob(job_id.to_string()))?
            .job
            .clone();
        if current.commit_in_progress
            && matches!(to, RuntimeJobState::Cancelling | RuntimeJobState::Cancelled)
        {
            return Err(RuntimeJobError::CancellationTooLate(job_id.to_string()));
        }
        if !valid_transition(current.state, to) {
            return Err(RuntimeJobError::IllegalTransition {
                job_id: job_id.to_string(),
                from: current.state,
                to,
            });
        }
        let terminal = to.is_terminal();
        if terminal != terminal_result.is_some() {
            return Err(RuntimeJobError::refused(
                "terminal_result_mismatch",
                "terminal transitions require exactly one terminal result",
            ));
        }
        let now = now_unix_ms()?;
        let mut next = current;
        next.state = to;
        next.revision = next.revision.saturating_add(1);
        next.updated_at_unix_ms = now;
        next.running_after_timeout |= running_after_timeout;
        if terminal {
            next.commit_in_progress = false;
        }
        next.state_reason = Some(reason.into());
        next.terminal_result = terminal_result;
        self.append_then_replace(&mut state, next.clone())?;
        Ok(next)
    }

    fn request_cancel(
        &self,
        job_id: &str,
        reason: &str,
        running_after_timeout: bool,
    ) -> Result<RuntimeJobV1, RuntimeJobError> {
        let mut state = self.lock()?;
        let entry = state
            .jobs
            .get(job_id)
            .ok_or_else(|| RuntimeJobError::UnknownJob(job_id.to_string()))?;
        if entry.job.state.is_terminal() {
            return Ok(entry.job.clone());
        }
        let current = entry.job.clone();
        let token = entry.cancellation.clone();
        if current.commit_in_progress {
            return Err(RuntimeJobError::CancellationTooLate(job_id.to_string()));
        }
        if current.state == RuntimeJobState::Cancelling {
            token.cancel();
            if !running_after_timeout || current.running_after_timeout {
                return Ok(current);
            }
            let now = now_unix_ms()?;
            let mut next = current;
            next.revision = next.revision.saturating_add(1);
            next.updated_at_unix_ms = now;
            next.running_after_timeout = true;
            next.state_reason = Some(reason.to_string());
            self.append_then_replace(&mut state, next.clone())?;
            return Ok(next);
        }
        let now = now_unix_ms()?;
        let mut next = current;
        next.state = RuntimeJobState::Cancelling;
        next.revision = next.revision.saturating_add(1);
        next.updated_at_unix_ms = now;
        next.running_after_timeout |= running_after_timeout;
        next.state_reason = Some(reason.to_string());
        self.append_then_replace(&mut state, next.clone())?;
        token.cancel();
        Ok(next)
    }

    fn finish_after_prepare<P, C>(
        &self,
        job_id: &str,
        prepared: Result<P, RuntimeJobFailure>,
        commit: C,
    ) where
        C: FnOnce(P) -> Result<RuntimeJobSuccess, RuntimeJobFailure>,
    {
        let mut state = match self.lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        let current = match state.jobs.get(job_id) {
            Some(entry) => entry.job.clone(),
            None => return,
        };
        let now = now_unix_ms().unwrap_or(u64::MAX);

        if current.state == RuntimeJobState::Cancelling || now >= current.deadline_unix_ms {
            let timed_out = now >= current.deadline_unix_ms;
            let mut cancelling = current;
            if cancelling.state == RuntimeJobState::Running {
                cancelling.state = RuntimeJobState::Cancelling;
                cancelling.revision = cancelling.revision.saturating_add(1);
                cancelling.updated_at_unix_ms = now;
                cancelling.running_after_timeout |= timed_out;
                cancelling.state_reason = Some(if timed_out {
                    "deadline_reached".to_string()
                } else {
                    "cancellation_requested".to_string()
                });
                if self
                    .append_then_replace(&mut state, cancelling.clone())
                    .is_err()
                {
                    return;
                }
                if let Some(entry) = state.jobs.get(job_id) {
                    entry.cancellation.cancel();
                }
            }
            let terminal = RuntimeJobTerminalResultV1 {
                schema: RUNTIME_JOB_TERMINAL_RESULT_SCHEMA.to_string(),
                code: if timed_out {
                    "deadline_cancelled".to_string()
                } else {
                    "cancelled".to_string()
                },
                message: "prepared result was discarded; commit was not invoked".to_string(),
                output_digest: None,
                finished_at_unix_ms: now,
            };
            let latest = state
                .jobs
                .get(job_id)
                .expect("job remains registered")
                .job
                .clone();
            let mut next = latest;
            next.state = RuntimeJobState::Cancelled;
            next.revision = next.revision.saturating_add(1);
            next.updated_at_unix_ms = now;
            next.commit_in_progress = false;
            next.state_reason = Some("cleanup_confirmed_without_commit".to_string());
            next.terminal_result = Some(terminal);
            let _ = self.append_then_replace(&mut state, next);
            return;
        }

        if current.state != RuntimeJobState::Running {
            return;
        }

        let value = match prepared {
            Ok(value) => value,
            Err(failure) => {
                let terminal = RuntimeJobTerminalResultV1 {
                    schema: RUNTIME_JOB_TERMINAL_RESULT_SCHEMA.to_string(),
                    code: failure.code,
                    message: failure.message,
                    output_digest: None,
                    finished_at_unix_ms: now,
                };
                let mut next = current;
                next.state = RuntimeJobState::Failed;
                next.revision = next.revision.saturating_add(1);
                next.updated_at_unix_ms = now;
                next.commit_in_progress = false;
                next.state_reason = Some("preparation_failed".to_string());
                next.terminal_result = Some(terminal);
                let _ = self.append_then_replace(&mut state, next);
                return;
            }
        };

        // The durable commit reservation is the cancellation/commit
        // linearization point. The slow or externally serialized commit runs
        // only after this short critical section releases the registry lock.
        let mut reserved = current;
        reserved.revision = reserved.revision.saturating_add(1);
        reserved.updated_at_unix_ms = now;
        reserved.commit_in_progress = true;
        reserved.state_reason = Some("commit_reserved".to_string());
        if self.append_then_replace(&mut state, reserved).is_err() {
            return;
        }
        drop(state);

        let (terminal_state, success_or_failure) =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| commit(value))) {
                Ok(Ok(success)) => (RuntimeJobState::Succeeded, Ok(success)),
                Ok(Err(failure)) => (RuntimeJobState::Failed, Err(failure)),
                Err(_) => (
                    RuntimeJobState::Failed,
                    Err(RuntimeJobFailure::new(
                        "commit_panicked",
                        "registry-controlled commit closure panicked",
                    )),
                ),
            };
        let finished_at = now_unix_ms().unwrap_or(now);
        let terminal = match success_or_failure {
            Ok(success) => RuntimeJobTerminalResultV1 {
                schema: RUNTIME_JOB_TERMINAL_RESULT_SCHEMA.to_string(),
                code: success.code,
                message: success.message,
                output_digest: success.output_digest,
                finished_at_unix_ms: finished_at,
            },
            Err(failure) => RuntimeJobTerminalResultV1 {
                schema: RUNTIME_JOB_TERMINAL_RESULT_SCHEMA.to_string(),
                code: failure.code,
                message: failure.message,
                output_digest: None,
                finished_at_unix_ms: finished_at,
            },
        };
        let mut state = match self.lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        let latest = match state.jobs.get(job_id) {
            Some(entry)
                if entry.job.state == RuntimeJobState::Running && entry.job.commit_in_progress =>
            {
                entry.job.clone()
            }
            _ => return,
        };
        let completed_after_deadline = finished_at >= latest.deadline_unix_ms;
        let mut next = latest;
        next.state = terminal_state;
        next.revision = next.revision.saturating_add(1);
        next.updated_at_unix_ms = terminal.finished_at_unix_ms;
        next.running_after_timeout |= completed_after_deadline;
        next.commit_in_progress = false;
        next.state_reason = Some(if terminal_state == RuntimeJobState::Succeeded {
            if completed_after_deadline {
                "commit_completed_after_deadline".to_string()
            } else {
                "commit_completed".to_string()
            }
        } else if completed_after_deadline {
            "commit_failed_after_deadline".to_string()
        } else {
            "commit_failed".to_string()
        });
        next.terminal_result = Some(terminal);
        let _ = self.append_then_replace(&mut state, next);
    }

    fn finish_panicked(&self, job_id: &str) {
        let current = match self.snapshot(job_id) {
            Ok(job) => job,
            Err(_) => return,
        };
        if current.state.is_terminal() {
            return;
        }
        let terminal = RuntimeJobTerminalResultV1 {
            schema: RUNTIME_JOB_TERMINAL_RESULT_SCHEMA.to_string(),
            code: "worker_panicked".to_string(),
            message: "preparation worker panicked; cleanup cannot be proven".to_string(),
            output_digest: None,
            finished_at_unix_ms: now_unix_ms().unwrap_or(u64::MAX),
        };
        let _ = self.transition(
            job_id,
            RuntimeJobState::Failed,
            "worker_panicked",
            current.running_after_timeout,
            Some(terminal),
        );
    }

    fn snapshot(&self, job_id: &str) -> Result<RuntimeJobV1, RuntimeJobError> {
        let state = self.lock()?;
        state
            .jobs
            .get(job_id)
            .map(|entry| observable_job(entry.job.clone()))
            .ok_or_else(|| RuntimeJobError::UnknownJob(job_id.to_string()))
    }
}

struct RegistryShared {
    core: Arc<RegistryCore>,
    tasks: Mutex<HashMap<String, JoinHandle<()>>>,
}

impl Drop for RegistryShared {
    fn drop(&mut self) {
        cancel_all_for_drop(&self.core);
        let handles = match self.tasks.get_mut() {
            Ok(tasks) => tasks.drain().map(|(_, handle)| handle).collect::<Vec<_>>(),
            Err(poisoned) => poisoned
                .into_inner()
                .drain()
                .map(|(_, handle)| handle)
                .collect::<Vec<_>>(),
        };
        for handle in handles {
            let _ = handle.join();
        }
    }
}

#[derive(Clone)]
pub struct RuntimeJobRegistry {
    shared: Arc<RegistryShared>,
}

impl RuntimeJobRegistry {
    pub fn open(journal_path: impl AsRef<Path>) -> Result<Self, RuntimeJobError> {
        Self::open_with_max_in_flight(journal_path, DEFAULT_MAX_IN_FLIGHT_JOBS)
    }

    pub fn open_with_max_in_flight(
        journal_path: impl AsRef<Path>,
        max_in_flight: usize,
    ) -> Result<Self, RuntimeJobError> {
        if max_in_flight == 0 {
            return Err(RuntimeJobError::refused(
                "invalid_max_in_flight",
                "max_in_flight must be greater than zero",
            ));
        }
        let (journal, replayed) = JournalWriter::open(journal_path.as_ref())?;
        let mut jobs = BTreeMap::new();
        let mut idempotency = HashMap::new();
        for (job_id, job) in replayed {
            if let Some(existing) = idempotency.insert(job.idempotency_key.clone(), job_id.clone())
            {
                return Err(RuntimeJobError::Corruption {
                    offset: 0,
                    detail: format!(
                        "idempotency key '{}' belongs to both '{}' and '{}'",
                        job.idempotency_key, existing, job_id
                    ),
                });
            }
            jobs.insert(
                job_id,
                JobEntry {
                    job,
                    cancellation: CancellationToken::new(),
                },
            );
        }
        let registry = Self {
            shared: Arc::new(RegistryShared {
                core: Arc::new(RegistryCore {
                    state: Mutex::new(RegistryState {
                        accepting: true,
                        poisoned: false,
                        max_in_flight,
                        jobs,
                        idempotency,
                        journal,
                    }),
                    changed: Condvar::new(),
                }),
                tasks: Mutex::new(HashMap::new()),
            }),
        };
        registry.fail_closed_interrupted_jobs()?;
        Ok(registry)
    }

    pub fn submit_prepared<P, Prepare, Commit>(
        &self,
        request: RuntimeJobRequestV1,
        prepare: Prepare,
        commit: Commit,
    ) -> Result<String, RuntimeJobError>
    where
        P: Send + 'static,
        Prepare: FnOnce(RuntimeJobContext) -> Result<P, RuntimeJobFailure> + Send + 'static,
        Commit: FnOnce(P) -> Result<RuntimeJobSuccess, RuntimeJobFailure> + Send + 'static,
    {
        validate_request(&request)?;
        let job_id = request.job_id.clone();
        let token = self.register_pending(request)?;
        self.shared.core.transition(
            &job_id,
            RuntimeJobState::Running,
            "worker_starting",
            false,
            None,
        )?;

        let core = Arc::clone(&self.shared.core);
        let worker_job_id = job_id.clone();
        let context_job = core.snapshot(&job_id)?;
        let context = RuntimeJobContext {
            job_id: job_id.clone(),
            binding: context_job.binding,
            snapshot_revision: context_job.snapshot_revision,
            deadline_unix_ms: context_job.deadline_unix_ms,
            cancellation: token,
        };
        let controller = thread::Builder::new()
            .name("m1nd-runtime-job".to_string())
            .spawn(move || {
                supervise_operation(core, worker_job_id, context, prepare, commit);
            })
            .map_err(|error| {
                let terminal = RuntimeJobTerminalResultV1 {
                    schema: RUNTIME_JOB_TERMINAL_RESULT_SCHEMA.to_string(),
                    code: "worker_spawn_failed".to_string(),
                    message: error.to_string(),
                    output_digest: None,
                    finished_at_unix_ms: now_unix_ms().unwrap_or(u64::MAX),
                };
                let _ = self.shared.core.transition(
                    &job_id,
                    RuntimeJobState::Failed,
                    "worker_spawn_failed",
                    false,
                    Some(terminal),
                );
                RuntimeJobError::Spawn(error)
            })?;

        let mut tasks = self
            .shared
            .tasks
            .lock()
            .map_err(|_| RuntimeJobError::RegistryPoisoned)?;
        tasks.insert(job_id.clone(), controller);
        Ok(job_id)
    }

    pub fn request_cancel(&self, job_id: &str) -> Result<RuntimeJobV1, RuntimeJobError> {
        self.shared
            .core
            .request_cancel(job_id, "explicit_cancellation_requested", false)
    }

    pub fn get(&self, job_id: &str) -> Result<RuntimeJobV1, RuntimeJobError> {
        self.shared.core.snapshot(job_id)
    }

    pub fn list(&self) -> Result<Vec<RuntimeJobV1>, RuntimeJobError> {
        let state = self.shared.core.lock()?;
        Ok(state
            .jobs
            .values()
            .map(|entry| observable_job(entry.job.clone()))
            .collect())
    }

    pub fn health_snapshot(&self) -> Result<RuntimeJobHealthV1, RuntimeJobError> {
        let state = self.shared.core.lock()?;
        let mut state_counts = BTreeMap::new();
        let mut active_jobs = 0;
        let mut running_after_timeout = 0;
        let mut commit_in_progress = 0;
        let now = now_unix_ms().unwrap_or(u64::MAX);
        for entry in state.jobs.values() {
            *state_counts
                .entry(format!("{:?}", entry.job.state).to_ascii_uppercase())
                .or_insert(0) += 1;
            if !entry.job.state.is_terminal() {
                active_jobs += 1;
            }
            if (entry.job.running_after_timeout || now >= entry.job.deadline_unix_ms)
                && !entry.job.state.is_terminal()
            {
                running_after_timeout += 1;
            }
            if entry.job.commit_in_progress {
                commit_in_progress += 1;
            }
        }
        Ok(RuntimeJobHealthV1 {
            schema: RUNTIME_JOB_HEALTH_SCHEMA.to_string(),
            accepting: state.accepting,
            poisoned: state.poisoned,
            max_in_flight: state.max_in_flight,
            active_jobs,
            total_jobs: state.jobs.len(),
            running_after_timeout,
            commit_in_progress,
            state_counts,
        })
    }

    pub fn wait_terminal(
        &self,
        job_id: &str,
        timeout: Duration,
    ) -> Result<RuntimeJobWait, RuntimeJobError> {
        let state = self.shared.core.lock()?;
        if !state.jobs.contains_key(job_id) {
            return Err(RuntimeJobError::UnknownJob(job_id.to_string()));
        }
        let (state, _) = self
            .shared
            .core
            .changed
            .wait_timeout_while(state, timeout, |state| {
                state
                    .jobs
                    .get(job_id)
                    .is_some_and(|entry| !entry.job.state.is_terminal())
            })
            .map_err(|_| RuntimeJobError::RegistryPoisoned)?;
        let job = state
            .jobs
            .get(job_id)
            .expect("job existence checked before wait")
            .job
            .clone();
        drop(state);
        let job = observable_job(job);
        self.reap_finished()?;
        if job.state.is_terminal() {
            Ok(RuntimeJobWait::Terminal(job))
        } else {
            Ok(RuntimeJobWait::ObservableNonTerminal(job))
        }
    }

    /// Close job submission without cancelling or joining workers. The owner
    /// lifecycle uses this at the same linearization point as its transport
    /// admission fence so even a previously retained registry clone cannot
    /// submit in the drain-to-shutdown window.
    pub(crate) fn close_admission(&self) -> Result<(), RuntimeJobError> {
        let mut state = self.shared.core.lock()?;
        state.accepting = false;
        self.shared.core.changed.notify_all();
        Ok(())
    }

    pub fn shutdown(&self, grace: Duration) -> Result<(), RuntimeJobError> {
        let active = {
            let mut state = self.shared.core.lock()?;
            state.accepting = false;
            state
                .jobs
                .iter()
                .filter(|(_, entry)| !entry.job.state.is_terminal())
                .map(|(job_id, _)| job_id.clone())
                .collect::<Vec<_>>()
        };
        for job_id in active {
            let snapshot = self.shared.core.snapshot(&job_id)?;
            if snapshot.state == RuntimeJobState::Pending {
                let terminal = RuntimeJobTerminalResultV1 {
                    schema: RUNTIME_JOB_TERMINAL_RESULT_SCHEMA.to_string(),
                    code: "shutdown_before_start".to_string(),
                    message: "job was cancelled before execution during shutdown".to_string(),
                    output_digest: None,
                    finished_at_unix_ms: now_unix_ms()?,
                };
                self.shared.core.transition(
                    &job_id,
                    RuntimeJobState::Cancelled,
                    "shutdown_before_start",
                    false,
                    Some(terminal),
                )?;
            } else {
                match self
                    .shared
                    .core
                    .request_cancel(&job_id, "shutdown_requested", false)
                {
                    Ok(_) | Err(RuntimeJobError::CancellationTooLate(_)) => {}
                    Err(error) => return Err(error),
                }
            }
        }

        let state = self.shared.core.lock()?;
        let (state, _) = self
            .shared
            .core
            .changed
            .wait_timeout_while(state, grace, |state| {
                state
                    .jobs
                    .values()
                    .any(|entry| !entry.job.state.is_terminal())
            })
            .map_err(|_| RuntimeJobError::RegistryPoisoned)?;
        let active_job_ids = state
            .jobs
            .iter()
            .filter(|(_, entry)| !entry.job.state.is_terminal())
            .map(|(job_id, _)| job_id.clone())
            .collect::<Vec<_>>();
        drop(state);
        self.reap_finished()?;
        if active_job_ids.is_empty() {
            Ok(())
        } else {
            Err(RuntimeJobError::ShutdownIncomplete(ShutdownIncomplete {
                active_job_ids,
            }))
        }
    }

    pub fn journal_path(&self) -> Result<PathBuf, RuntimeJobError> {
        let state = self.shared.core.lock()?;
        Ok(state.journal.path.clone())
    }

    fn register_pending(
        &self,
        request: RuntimeJobRequestV1,
    ) -> Result<CancellationToken, RuntimeJobError> {
        let mut state = self.shared.core.lock()?;
        if state.poisoned {
            return Err(RuntimeJobError::RegistryPoisoned);
        }
        if !state.accepting {
            return Err(RuntimeJobError::RegistryShuttingDown);
        }
        let active = state
            .jobs
            .values()
            .filter(|entry| !entry.job.state.is_terminal())
            .count();
        if active >= state.max_in_flight {
            return Err(RuntimeJobError::Overloaded {
                limit: state.max_in_flight,
                active,
            });
        }
        if state.jobs.contains_key(&request.job_id) {
            return Err(RuntimeJobError::DuplicateJobId(request.job_id));
        }
        if state.idempotency.contains_key(&request.idempotency_key) {
            return Err(RuntimeJobError::DuplicateIdempotencyKey(
                request.idempotency_key,
            ));
        }

        let now = now_unix_ms()?;
        let job = RuntimeJobV1 {
            schema: RUNTIME_JOB_SCHEMA.to_string(),
            job_id: request.job_id.clone(),
            idempotency_key: request.idempotency_key.clone(),
            binding: request.binding,
            snapshot_revision: request.snapshot_revision,
            deadline_unix_ms: request.deadline_unix_ms,
            state: RuntimeJobState::Pending,
            revision: 0,
            registered_at_unix_ms: now,
            updated_at_unix_ms: now,
            running_after_timeout: false,
            commit_in_progress: false,
            state_reason: Some("registered_before_execution".to_string()),
            terminal_result: None,
        };
        if let Err(error) = state.journal.append(job.clone()) {
            state.poisoned = true;
            return Err(error);
        }
        let token = CancellationToken::new();
        state
            .idempotency
            .insert(request.idempotency_key, request.job_id.clone());
        state.jobs.insert(
            request.job_id,
            JobEntry {
                job,
                cancellation: token.clone(),
            },
        );
        self.shared.core.changed.notify_all();
        Ok(token)
    }

    fn fail_closed_interrupted_jobs(&self) -> Result<(), RuntimeJobError> {
        let interrupted = {
            let state = self.shared.core.lock()?;
            state
                .jobs
                .iter()
                .filter(|(_, entry)| !entry.job.state.is_terminal())
                .map(|(job_id, _)| job_id.clone())
                .collect::<Vec<_>>()
        };
        for job_id in interrupted {
            let current = self.shared.core.snapshot(&job_id)?;
            let commit_was_reserved = current.commit_in_progress;
            let terminal = RuntimeJobTerminalResultV1 {
                schema: RUNTIME_JOB_TERMINAL_RESULT_SCHEMA.to_string(),
                code: if commit_was_reserved {
                    "restart_interrupted_commit_ambiguous".to_string()
                } else {
                    "restart_interrupted".to_string()
                },
                message: if commit_was_reserved {
                    "restart found a durable commit reservation; external commit outcome was not inferred"
                        .to_string()
                } else {
                    "process restart interrupted a non-terminal job; no commit was inferred"
                        .to_string()
                },
                output_digest: None,
                finished_at_unix_ms: now_unix_ms()?,
            };
            let target = if current.state == RuntimeJobState::Pending {
                RuntimeJobState::Failed
            } else if current.state == RuntimeJobState::Running && !commit_was_reserved {
                self.shared.core.request_cancel(
                    &job_id,
                    "restart_interrupted_running_job",
                    current.running_after_timeout,
                )?;
                RuntimeJobState::Failed
            } else {
                RuntimeJobState::Failed
            };
            self.shared.core.transition(
                &job_id,
                target,
                "restart_failed_closed",
                current.running_after_timeout,
                Some(terminal),
            )?;
        }
        Ok(())
    }

    fn reap_finished(&self) -> Result<(), RuntimeJobError> {
        let handles = {
            let mut tasks = self
                .shared
                .tasks
                .lock()
                .map_err(|_| RuntimeJobError::RegistryPoisoned)?;
            let finished = tasks
                .iter()
                .filter(|(_, handle)| handle.is_finished())
                .map(|(job_id, _)| job_id.clone())
                .collect::<Vec<_>>();
            finished
                .into_iter()
                .filter_map(|job_id| tasks.remove(&job_id))
                .collect::<Vec<_>>()
        };
        for handle in handles {
            let _ = handle.join();
        }
        Ok(())
    }
}

fn supervise_operation<P, Prepare, Commit>(
    core: Arc<RegistryCore>,
    job_id: String,
    context: RuntimeJobContext,
    prepare: Prepare,
    commit: Commit,
) where
    P: Send + 'static,
    Prepare: FnOnce(RuntimeJobContext) -> Result<P, RuntimeJobFailure> + Send + 'static,
    Commit: FnOnce(P) -> Result<RuntimeJobSuccess, RuntimeJobFailure> + Send + 'static,
{
    let deadline = context.deadline_unix_ms;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let worker = match thread::Builder::new()
        .name("m1nd-runtime-prepare".to_string())
        .spawn(move || {
            let result = prepare(context);
            let _ = sender.send(result);
        }) {
        Ok(worker) => worker,
        Err(error) => {
            let terminal = RuntimeJobTerminalResultV1 {
                schema: RUNTIME_JOB_TERMINAL_RESULT_SCHEMA.to_string(),
                code: "prepare_spawn_failed".to_string(),
                message: error.to_string(),
                output_digest: None,
                finished_at_unix_ms: now_unix_ms().unwrap_or(u64::MAX),
            };
            let _ = core.transition(
                &job_id,
                RuntimeJobState::Failed,
                "prepare_spawn_failed",
                false,
                Some(terminal),
            );
            return;
        }
    };

    let remaining = deadline.saturating_sub(now_unix_ms().unwrap_or(u64::MAX));
    let prepared = match receiver.recv_timeout(Duration::from_millis(remaining)) {
        Ok(result) => Some(result),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            let _ = core.request_cancel(&job_id, "deadline_reached", true);
            receiver.recv().ok()
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => None,
    };

    let joined = worker.join();
    if joined.is_err() || prepared.is_none() {
        core.finish_panicked(&job_id);
        return;
    }
    core.finish_after_prepare(&job_id, prepared.expect("checked Some"), commit);
}

fn cancel_all_for_drop(core: &RegistryCore) {
    let active = match core.lock() {
        Ok(mut state) => {
            state.accepting = false;
            state
                .jobs
                .iter()
                .filter(|(_, entry)| !entry.job.state.is_terminal())
                .map(|(job_id, _)| job_id.clone())
                .collect::<Vec<_>>()
        }
        Err(_) => return,
    };
    for job_id in active {
        let _ = core.request_cancel(&job_id, "registry_drop_requested", false);
    }
}

fn validate_request(request: &RuntimeJobRequestV1) -> Result<(), RuntimeJobError> {
    validate_identifier("job_id", &request.job_id)?;
    validate_identifier("idempotency_key", &request.idempotency_key)?;
    validate_binding(&request.binding)?;
    if request.deadline_unix_ms == 0 {
        return Err(RuntimeJobError::refused(
            "invalid_deadline",
            "deadline_unix_ms must be non-zero",
        ));
    }
    Ok(())
}

fn validate_job_shape(job: &RuntimeJobV1) -> Result<(), RuntimeJobError> {
    if job.schema != RUNTIME_JOB_SCHEMA {
        return Err(RuntimeJobError::refused(
            "unsupported_job_schema",
            format!("observed '{}'", job.schema),
        ));
    }
    validate_identifier("job_id", &job.job_id)?;
    validate_identifier("idempotency_key", &job.idempotency_key)?;
    validate_binding(&job.binding)?;
    if job.state.is_terminal() != job.terminal_result.is_some() {
        return Err(RuntimeJobError::refused(
            "terminal_result_mismatch",
            "terminal state and terminal result presence disagree",
        ));
    }
    if job.commit_in_progress && job.state != RuntimeJobState::Running {
        return Err(RuntimeJobError::refused(
            "invalid_commit_reservation",
            "commit_in_progress is only valid while state is RUNNING",
        ));
    }
    if let Some(terminal) = &job.terminal_result {
        if terminal.schema != RUNTIME_JOB_TERMINAL_RESULT_SCHEMA {
            return Err(RuntimeJobError::refused(
                "unsupported_terminal_result_schema",
                format!("observed '{}'", terminal.schema),
            ));
        }
        validate_identifier("terminal.code", &terminal.code)?;
    }
    Ok(())
}

fn validate_binding(binding: &RuntimeJobBindingV1) -> Result<(), RuntimeJobError> {
    if binding.schema != RUNTIME_JOB_BINDING_SCHEMA {
        return Err(RuntimeJobError::refused(
            "unsupported_job_binding_schema",
            format!("observed '{}'", binding.schema),
        ));
    }
    for (field, value) in [
        ("organism_id", binding.organism_id.as_str()),
        ("brain_id", binding.brain_id.as_str()),
        ("mission_id", binding.mission_id.as_str()),
        ("agent_id", binding.agent_id.as_str()),
        ("action", binding.action.as_str()),
    ] {
        validate_identifier(field, value)?;
    }
    if binding.effects.is_empty() {
        return Err(RuntimeJobError::refused(
            "empty_effect_binding",
            "runtime job must bind at least one complete effect",
        ));
    }
    if binding.authority.schema != RUNTIME_JOB_AUTHORITY_SCHEMA {
        return Err(RuntimeJobError::refused(
            "unsupported_job_authority_schema",
            format!("observed '{}'", binding.authority.schema),
        ));
    }
    validate_identifier("decision_id", &binding.authority.decision_id)?;
    validate_identifier(
        "authorization_digest",
        &binding.authority.authorization_digest,
    )?;
    if let Some(capability_id) = &binding.authority.capability_id {
        validate_identifier("capability_id", capability_id)?;
    }
    Ok(())
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), RuntimeJobError> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(RuntimeJobError::refused(
            "invalid_identifier",
            format!("{field} must contain 1..=256 non-control characters"),
        ));
    }
    Ok(())
}

fn observable_job(mut job: RuntimeJobV1) -> RuntimeJobV1 {
    if !job.state.is_terminal() && now_unix_ms().unwrap_or(u64::MAX) >= job.deadline_unix_ms {
        job.running_after_timeout = true;
    }
    job
}

fn now_unix_ms() -> Result<u64, RuntimeJobError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| RuntimeJobError::refused("clock_before_epoch", error.to_string()))?;
    Ok(duration.as_millis().try_into().unwrap_or(u64::MAX))
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
fn lock_exclusive(file: &File) -> Result<(), RuntimeJobError> {
    use std::os::fd::AsRawFd;
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        Ok(())
    } else {
        Err(RuntimeJobError::refused(
            "journal_writer_lock_refused",
            io::Error::last_os_error().to_string(),
        ))
    }
}

#[cfg(not(unix))]
fn lock_exclusive(_file: &File) -> Result<(), RuntimeJobError> {
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
fn sync_parent_directory(parent: &Path) -> Result<(), RuntimeJobError> {
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> Result<(), RuntimeJobError> {
    Ok(())
}

#[cfg(test)]
mod internal_tests {
    use super::*;

    fn binding() -> RuntimeJobBindingV1 {
        RuntimeJobBindingV1 {
            schema: RUNTIME_JOB_BINDING_SCHEMA.to_string(),
            organism_id: "org-test".to_string(),
            brain_id: "brain-test".to_string(),
            mission_id: "mission-test".to_string(),
            agent_id: "agent-test".to_string(),
            action: ActionId::new("graph.ingest").expect("action"),
            ingress: Ingress::BackgroundJob,
            effects: BTreeSet::from([Effect::GraphMutation]),
            authority: RuntimeJobAuthorityBindingV1 {
                schema: RUNTIME_JOB_AUTHORITY_SCHEMA.to_string(),
                decision_id: "decision-test".to_string(),
                authority_variant: AuthorityVariant::Policy,
                authority_epoch: 7,
                autonomy_epoch: 3,
                capability_id: Some("cap-test".to_string()),
                authorization_digest: "a".repeat(64),
            },
        }
    }

    fn request(job_id: &str) -> RuntimeJobRequestV1 {
        RuntimeJobRequestV1 {
            job_id: job_id.to_string(),
            idempotency_key: format!("idem-{job_id}"),
            binding: binding(),
            snapshot_revision: 11,
            deadline_unix_ms: now_unix_ms().expect("clock") + 5_000,
        }
    }

    #[test]
    fn restart_fails_closed_non_terminal_job_and_persists_terminal_result() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("jobs.jsonl");
        let (mut journal, _) = JournalWriter::open(&path).expect("journal");
        let request = request("job-restart");
        let now = now_unix_ms().expect("clock");
        let pending = RuntimeJobV1 {
            schema: RUNTIME_JOB_SCHEMA.to_string(),
            job_id: request.job_id,
            idempotency_key: request.idempotency_key,
            binding: request.binding,
            snapshot_revision: request.snapshot_revision,
            deadline_unix_ms: request.deadline_unix_ms,
            state: RuntimeJobState::Pending,
            revision: 0,
            registered_at_unix_ms: now,
            updated_at_unix_ms: now,
            running_after_timeout: false,
            commit_in_progress: false,
            state_reason: Some("registered_before_execution".to_string()),
            terminal_result: None,
        };
        journal.append(pending.clone()).expect("pending append");
        let mut running = pending;
        running.state = RuntimeJobState::Running;
        running.revision = 1;
        running.state_reason = Some("worker_starting".to_string());
        journal.append(running).expect("running append");
        drop(journal);

        let registry = RuntimeJobRegistry::open(&path).expect("reopen");
        let recovered = registry.get("job-restart").expect("recovered job");
        assert_eq!(recovered.state, RuntimeJobState::Failed);
        assert_eq!(
            recovered
                .terminal_result
                .as_ref()
                .map(|result| result.code.as_str()),
            Some("restart_interrupted")
        );
        drop(registry);

        let reopened = RuntimeJobRegistry::open(&path).expect("second reopen");
        assert_eq!(
            reopened.get("job-restart").expect("job").state,
            RuntimeJobState::Failed
        );
    }

    #[test]
    fn restart_never_infers_outcome_after_durable_commit_reservation() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("commit-reserved.jsonl");
        let (mut journal, _) = JournalWriter::open(&path).expect("journal");
        let request = request("job-commit-reserved");
        let now = now_unix_ms().expect("clock");
        let pending = RuntimeJobV1 {
            schema: RUNTIME_JOB_SCHEMA.to_string(),
            job_id: request.job_id,
            idempotency_key: request.idempotency_key,
            binding: request.binding,
            snapshot_revision: request.snapshot_revision,
            deadline_unix_ms: request.deadline_unix_ms,
            state: RuntimeJobState::Pending,
            revision: 0,
            registered_at_unix_ms: now,
            updated_at_unix_ms: now,
            running_after_timeout: false,
            commit_in_progress: false,
            state_reason: Some("registered_before_execution".to_string()),
            terminal_result: None,
        };
        journal.append(pending.clone()).expect("pending");
        let mut running = pending;
        running.state = RuntimeJobState::Running;
        running.revision = 1;
        running.state_reason = Some("worker_starting".to_string());
        journal.append(running.clone()).expect("running");
        let mut reserved = running;
        reserved.revision = 2;
        reserved.commit_in_progress = true;
        reserved.state_reason = Some("commit_reserved".to_string());
        journal.append(reserved).expect("reservation");
        drop(journal);

        let registry = RuntimeJobRegistry::open(&path).expect("recovery");
        let recovered = registry.get("job-commit-reserved").expect("job");
        assert_eq!(recovered.state, RuntimeJobState::Failed);
        assert!(!recovered.commit_in_progress);
        assert_eq!(
            recovered
                .terminal_result
                .as_ref()
                .map(|result| result.code.as_str()),
            Some("restart_interrupted_commit_ambiguous")
        );
    }

    #[test]
    fn torn_tail_and_digest_tamper_fail_closed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let torn = temp.path().join("torn.jsonl");
        fs::write(&torn, b"{\"partial\":true}").expect("write torn");
        assert!(matches!(
            RuntimeJobRegistry::open(&torn),
            Err(RuntimeJobError::Corruption { .. })
        ));

        let tampered = temp.path().join("tampered.jsonl");
        let (mut journal, _) = JournalWriter::open(&tampered).expect("journal");
        let request = request("job-tamper");
        let now = now_unix_ms().expect("clock");
        journal
            .append(RuntimeJobV1 {
                schema: RUNTIME_JOB_SCHEMA.to_string(),
                job_id: request.job_id,
                idempotency_key: request.idempotency_key,
                binding: request.binding,
                snapshot_revision: request.snapshot_revision,
                deadline_unix_ms: request.deadline_unix_ms,
                state: RuntimeJobState::Pending,
                revision: 0,
                registered_at_unix_ms: now,
                updated_at_unix_ms: now,
                running_after_timeout: false,
                commit_in_progress: false,
                state_reason: None,
                terminal_result: None,
            })
            .expect("append");
        drop(journal);
        let bytes = fs::read(&tampered).expect("read");
        let mut text = String::from_utf8(bytes).expect("utf8");
        text = text.replacen("brain-test", "brain-evil", 1);
        fs::write(&tampered, text).expect("tamper");
        assert!(matches!(
            RuntimeJobRegistry::open(&tampered),
            Err(RuntimeJobError::Corruption { .. })
        ));
    }
}
