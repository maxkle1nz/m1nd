//! M1ND-10 G5 — durable evidence correlation spine.
//!
//! This store is a projection, never a new domain authority. `ReceiptV1` remains
//! evidence authority, the MissionService letter chain remains mission-state
//! authority, delegation records remain delegation authority, and Mission
//! Control remains the reasoning trail. The spine verifies their canonical
//! digests and exact bindings, then appends a hash-chained correlation event that
//! can be rebuilt into a Mission-Control-friendly read model after restart.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs::File;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use m1nd_control::{digest_canonical, CanonicalError, MissionState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::mission_service::{
    EvidenceRefV1, MissionLetterV1, ReceiptCandidateV1, ReceiptV1, EVIDENCE_REF_SCHEMA,
    MISSION_LETTER_V1_SCHEMA, RECEIPT_CANDIDATE_SCHEMA, RECEIPT_SCHEMA,
};

pub const EVIDENCE_SPINE_IDENTITY_SCHEMA: &str = "m1nd-evidence-spine-identity-v1";
pub const EVIDENCE_MISSION_BINDING_SCHEMA: &str = "m1nd-evidence-mission-binding-v1";
pub const EVIDENCE_SPINE_EVENT_SCHEMA: &str = "m1nd-evidence-spine-event-v1";
pub const EVIDENCE_SPINE_ROW_SCHEMA: &str = "m1nd-evidence-spine-row-v1";
pub const EVIDENCE_CORRELATION_READ_MODEL_SCHEMA: &str = "m1nd-evidence-correlation-read-model-v1";
pub const EVIDENCE_SPINE_QUERY_RESULT_SCHEMA: &str = "m1nd-evidence-spine-query-result-v1";
pub const EVIDENCE_CORRELATION_LINK_SCHEMA: &str = "m1nd-evidence-correlation-link-v1";
pub const EVIDENCE_SPINE_LOG_FILE: &str = "correlations.jsonl";

const EVIDENCE_SPINE_IDENTITY_FILE: &str = "identity.json";
const IDENTITY_DIGEST_DOMAIN: &str = "m1nd-evidence-spine-identity-v1";
const CORRELATION_ID_DOMAIN: &str = "m1nd-evidence-correlation-v1";
const SOURCE_DIGEST_DOMAIN: &str = "m1nd-evidence-spine-source-v1";
const EVENT_ID_DOMAIN: &str = "m1nd-evidence-spine-event-id-v1";
const ROW_DIGEST_DOMAIN: &str = "m1nd-evidence-spine-row-v1";
const IDENTITY_SET_DIGEST_DOMAIN: &str = "m1nd-evidence-identity-bindings-v1";
const EVIDENCE_SET_DIGEST_DOMAIN: &str = "m1nd-evidence-set-v1";
const DELEGATION_PACKET_DIGEST_DOMAIN: &str = "m1nd-delegation-packet-correlation-v1";
const DELEGATION_EVIDENCE_DIGEST_DOMAIN: &str = "m1nd-delegation-evidence-v1";
const MISSION_CONTROL_RECORD_DIGEST_DOMAIN: &str = "m1nd-mission-control-record-v1";
const MISSION_CONTROL_EVIDENCE_DIGEST_DOMAIN: &str = "m1nd-mission-control-evidence-v1";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum EvidenceSpineError {
    Io(io::Error),
    Json(serde_json::Error),
    Canonical(CanonicalError),
    Refused { code: &'static str, detail: String },
    Corruption { detail: String },
}

impl EvidenceSpineError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "evidence_spine_io",
            Self::Json(_) => "evidence_spine_json",
            Self::Canonical(_) => "evidence_spine_canonicalization",
            Self::Refused { code, .. } => code,
            Self::Corruption { .. } => "evidence_spine_corruption",
        }
    }

    fn refused(code: &'static str, detail: impl Into<String>) -> Self {
        Self::Refused {
            code,
            detail: detail.into(),
        }
    }

    fn corruption(detail: impl Into<String>) -> Self {
        Self::Corruption {
            detail: detail.into(),
        }
    }
}

impl fmt::Display for EvidenceSpineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "evidence spine I/O error: {error}"),
            Self::Json(error) => write!(formatter, "evidence spine JSON error: {error}"),
            Self::Canonical(error) => {
                write!(formatter, "evidence spine canonicalization error: {error}")
            }
            Self::Refused { code, detail } => write!(formatter, "{code}: {detail}"),
            Self::Corruption { detail } => write!(formatter, "evidence_spine_corruption: {detail}"),
        }
    }
}

impl Error for EvidenceSpineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Canonical(error) => Some(error),
            Self::Refused { .. } | Self::Corruption { .. } => None,
        }
    }
}

impl From<io::Error> for EvidenceSpineError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for EvidenceSpineError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<CanonicalError> for EvidenceSpineError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

pub type EvidenceSpineResult<T> = Result<T, EvidenceSpineError>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSpineIdentityV1 {
    pub schema: String,
    pub organism_id: String,
    pub brain_id: String,
    pub workspace_root: String,
    pub identity_digest: String,
}

impl EvidenceSpineIdentityV1 {
    pub fn new(
        organism_id: impl Into<String>,
        brain_id: impl Into<String>,
        workspace_root: impl AsRef<Path>,
    ) -> EvidenceSpineResult<Self> {
        let workspace_root = canonical_existing_directory(workspace_root.as_ref())?;
        let mut identity = Self {
            schema: EVIDENCE_SPINE_IDENTITY_SCHEMA.to_string(),
            organism_id: organism_id.into(),
            brain_id: brain_id.into(),
            workspace_root: path_string(&workspace_root)?,
            identity_digest: String::new(),
        };
        identity.validate_required()?;
        identity.identity_digest = identity.compute_digest()?;
        Ok(identity)
    }

    pub fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_without_fields(IDENTITY_DIGEST_DOMAIN, self, &["identity_digest"])
    }

    fn validate(&self) -> EvidenceSpineResult<()> {
        self.validate_required()?;
        require_digest("identity.identity_digest", &self.identity_digest)?;
        if self.compute_digest()? != self.identity_digest {
            return Err(EvidenceSpineError::corruption(
                "persisted evidence-spine identity digest does not match its canonical bytes",
            ));
        }
        let canonical = canonical_existing_directory(Path::new(&self.workspace_root))?;
        if path_string(&canonical)? != self.workspace_root {
            return Err(EvidenceSpineError::corruption(
                "persisted workspace_root is not its canonical filesystem identity",
            ));
        }
        Ok(())
    }

    fn validate_required(&self) -> EvidenceSpineResult<()> {
        require_schema(
            "evidence spine identity",
            &self.schema,
            EVIDENCE_SPINE_IDENTITY_SCHEMA,
        )?;
        require_non_empty("identity.organism_id", &self.organism_id)?;
        require_non_empty("identity.brain_id", &self.brain_id)?;
        require_non_empty("identity.workspace_root", &self.workspace_root)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceMissionBindingV1 {
    pub schema: String,
    pub organism_id: String,
    pub brain_id: String,
    pub mission_id: String,
    pub iteration_id: u64,
    pub workspace_root: String,
}

impl EvidenceMissionBindingV1 {
    pub fn new(
        identity: &EvidenceSpineIdentityV1,
        mission_id: impl Into<String>,
        iteration_id: u64,
    ) -> EvidenceSpineResult<Self> {
        identity.validate()?;
        let binding = Self {
            schema: EVIDENCE_MISSION_BINDING_SCHEMA.to_string(),
            organism_id: identity.organism_id.clone(),
            brain_id: identity.brain_id.clone(),
            mission_id: mission_id.into(),
            iteration_id,
            workspace_root: identity.workspace_root.clone(),
        };
        binding.validate(identity)?;
        Ok(binding)
    }

    pub fn correlation_id(&self) -> EvidenceSpineResult<String> {
        #[derive(Serialize)]
        struct CorrelationCore<'a> {
            organism_id: &'a str,
            brain_id: &'a str,
            mission_id: &'a str,
            iteration_id: u64,
        }
        let digest = digest_canonical(
            CORRELATION_ID_DOMAIN,
            &CorrelationCore {
                organism_id: &self.organism_id,
                brain_id: &self.brain_id,
                mission_id: &self.mission_id,
                iteration_id: self.iteration_id,
            },
        )?;
        Ok(format!("cor:{digest}"))
    }

    fn validate(&self, identity: &EvidenceSpineIdentityV1) -> EvidenceSpineResult<()> {
        require_schema(
            "evidence mission binding",
            &self.schema,
            EVIDENCE_MISSION_BINDING_SCHEMA,
        )?;
        require_non_empty("binding.organism_id", &self.organism_id)?;
        require_non_empty("binding.brain_id", &self.brain_id)?;
        require_non_empty("binding.mission_id", &self.mission_id)?;
        require_non_empty("binding.workspace_root", &self.workspace_root)?;
        if self.iteration_id == 0 {
            return Err(EvidenceSpineError::refused(
                "invalid_iteration",
                "evidence correlation requires iteration_id >= 1",
            ));
        }
        if self.organism_id != identity.organism_id
            || self.brain_id != identity.brain_id
            || self.workspace_root != identity.workspace_root
        {
            return Err(EvidenceSpineError::refused(
                "wrong_evidence_binding",
                "correlation binding does not match the opened organism/brain/workspace",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceCausalAttachmentV1 {
    pub mission_head_id: Option<String>,
    pub transaction_id: Option<String>,
}

impl EvidenceCausalAttachmentV1 {
    fn validate(&self) -> EvidenceSpineResult<()> {
        require_optional_non_empty(
            "attachment.mission_head_id",
            self.mission_head_id.as_deref(),
        )?;
        require_optional_non_empty("attachment.transaction_id", self.transaction_id.as_deref())
    }
}

/// Non-authoritative correlation token emitted by the owner after it has
/// observed a canonical G3 letter/receipt. Coordination callers may carry this
/// token back, but the spine rechecks that its exact head/transaction anchor
/// already exists before accepting delegation or Mission Control projections.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceCorrelationLinkV1 {
    pub schema: String,
    pub mission_id: String,
    pub iteration_id: u64,
    pub mission_head_id: String,
    pub transaction_id: Option<String>,
}

impl EvidenceCorrelationLinkV1 {
    pub fn new(
        mission_id: impl Into<String>,
        iteration_id: u64,
        mission_head_id: impl Into<String>,
        transaction_id: Option<String>,
    ) -> EvidenceSpineResult<Self> {
        let link = Self {
            schema: EVIDENCE_CORRELATION_LINK_SCHEMA.to_string(),
            mission_id: mission_id.into(),
            iteration_id,
            mission_head_id: mission_head_id.into(),
            transaction_id,
        };
        link.validate()?;
        Ok(link)
    }

    pub fn from_letter(letter: &MissionLetterV1) -> EvidenceSpineResult<Self> {
        validate_letter(letter)?;
        let mission_head_id = if letter.state == MissionState::Landed {
            letter.previous_head_id.clone().ok_or_else(|| {
                EvidenceSpineError::refused(
                    "landed_without_correlation",
                    "landed letter has no previous authority head",
                )
            })?
        } else {
            letter.head_id.clone()
        };
        Self::new(
            letter.mission_id.clone(),
            letter.iteration_id,
            mission_head_id,
            letter.transaction_id.clone(),
        )
    }

    pub fn binding(
        &self,
        identity: &EvidenceSpineIdentityV1,
    ) -> EvidenceSpineResult<EvidenceMissionBindingV1> {
        self.validate()?;
        EvidenceMissionBindingV1::new(identity, self.mission_id.clone(), self.iteration_id)
    }

    pub fn attachment(&self) -> EvidenceSpineResult<EvidenceCausalAttachmentV1> {
        self.validate()?;
        Ok(EvidenceCausalAttachmentV1 {
            mission_head_id: Some(self.mission_head_id.clone()),
            transaction_id: self.transaction_id.clone(),
        })
    }

    fn validate(&self) -> EvidenceSpineResult<()> {
        require_schema(
            "evidence correlation link",
            &self.schema,
            EVIDENCE_CORRELATION_LINK_SCHEMA,
        )?;
        require_non_empty("link.mission_id", &self.mission_id)?;
        require_non_empty("link.mission_head_id", &self.mission_head_id)?;
        require_optional_non_empty("link.transaction_id", self.transaction_id.as_deref())?;
        if self.iteration_id == 0 {
            return Err(EvidenceSpineError::refused(
                "invalid_iteration",
                "correlation link requires iteration_id >= 1",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EvidenceSpineSourceV1 {
    Receipt {
        receipt_id: String,
        receipt_digest: String,
        transaction_id: String,
        source_head_id: String,
        candidate_digest: String,
        block_id: String,
        evidence_refs_digest: String,
    },
    MissionLetter {
        head_id: String,
        previous_head_id: Option<String>,
        phase: MissionState,
        transaction_id: Option<String>,
        packet_digest: String,
        candidate_digest: Option<String>,
        committed_receipt_id: Option<String>,
        evidence_refs_digest: String,
    },
    DelegationPacket {
        delegation_id: String,
        packet_digest: String,
        agent_id: String,
        workspace_root: String,
    },
    DelegationOutcome {
        delegation_id: String,
        outcome_digest: String,
        grader_id: String,
        outcome: String,
        workspace_root: String,
    },
    MissionControlRecord {
        control_mission_id: String,
        record_digest: String,
        event_digest: String,
        agent_id: String,
        status: String,
        repo: String,
    },
}

impl EvidenceSpineSourceV1 {
    fn kind(&self) -> &'static str {
        match self {
            Self::Receipt { .. } => "receipt",
            Self::MissionLetter { .. } => "mission_letter",
            Self::DelegationPacket { .. } => "delegation_packet",
            Self::DelegationOutcome { .. } => "delegation_outcome",
            Self::MissionControlRecord { .. } => "mission_control_record",
        }
    }

    fn key(&self) -> &str {
        match self {
            Self::Receipt { receipt_id, .. } => receipt_id,
            Self::MissionLetter { head_id, .. } => head_id,
            Self::DelegationPacket { delegation_id, .. } => delegation_id,
            Self::DelegationOutcome { delegation_id, .. } => delegation_id,
            Self::MissionControlRecord {
                control_mission_id, ..
            } => control_mission_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSpineEventV1 {
    pub schema: String,
    pub event_id: String,
    pub correlation_id: String,
    pub organism_id: String,
    pub brain_id: String,
    pub mission_id: String,
    pub mission_head_id: Option<String>,
    pub iteration_id: u64,
    pub transaction_id: Option<String>,
    pub workspace_root: String,
    pub identity_digest: String,
    pub evidence_digest: String,
    pub source_digest: String,
    pub source: EvidenceSpineSourceV1,
    pub observed_at: u64,
}

impl EvidenceSpineEventV1 {
    fn build(
        binding: &EvidenceMissionBindingV1,
        attachment: EvidenceCausalAttachmentV1,
        identity_digest: String,
        evidence_digest: String,
        source: EvidenceSpineSourceV1,
        observed_at: u64,
    ) -> EvidenceSpineResult<Self> {
        attachment.validate()?;
        let source_digest = digest_canonical(SOURCE_DIGEST_DOMAIN, &source)?;
        let correlation_id = binding.correlation_id()?;
        let mut event = Self {
            schema: EVIDENCE_SPINE_EVENT_SCHEMA.to_string(),
            event_id: String::new(),
            correlation_id,
            organism_id: binding.organism_id.clone(),
            brain_id: binding.brain_id.clone(),
            mission_id: binding.mission_id.clone(),
            mission_head_id: attachment.mission_head_id,
            iteration_id: binding.iteration_id,
            transaction_id: attachment.transaction_id,
            workspace_root: binding.workspace_root.clone(),
            identity_digest,
            evidence_digest,
            source_digest,
            source,
            observed_at,
        };
        event.event_id = event.compute_event_id()?;
        Ok(event)
    }

    fn compute_event_id(&self) -> Result<String, CanonicalError> {
        #[derive(Serialize)]
        struct EventIdentity<'a> {
            correlation_id: &'a str,
            mission_head_id: &'a Option<String>,
            transaction_id: &'a Option<String>,
            identity_digest: &'a str,
            evidence_digest: &'a str,
            source_kind: &'a str,
            source_key: &'a str,
            source_digest: &'a str,
        }
        let digest = digest_canonical(
            EVENT_ID_DOMAIN,
            &EventIdentity {
                correlation_id: &self.correlation_id,
                mission_head_id: &self.mission_head_id,
                transaction_id: &self.transaction_id,
                identity_digest: &self.identity_digest,
                evidence_digest: &self.evidence_digest,
                source_kind: self.source.kind(),
                source_key: self.source.key(),
                source_digest: &self.source_digest,
            },
        )?;
        Ok(format!("evt:{digest}"))
    }

    fn validate(&self, identity: &EvidenceSpineIdentityV1) -> EvidenceSpineResult<()> {
        require_schema("evidence event", &self.schema, EVIDENCE_SPINE_EVENT_SCHEMA)?;
        require_non_empty("event.event_id", &self.event_id)?;
        require_non_empty("event.correlation_id", &self.correlation_id)?;
        require_digest("event.identity_digest", &self.identity_digest)?;
        require_digest("event.evidence_digest", &self.evidence_digest)?;
        require_digest("event.source_digest", &self.source_digest)?;
        require_optional_non_empty("event.mission_head_id", self.mission_head_id.as_deref())?;
        require_optional_non_empty("event.transaction_id", self.transaction_id.as_deref())?;

        let binding = EvidenceMissionBindingV1 {
            schema: EVIDENCE_MISSION_BINDING_SCHEMA.to_string(),
            organism_id: self.organism_id.clone(),
            brain_id: self.brain_id.clone(),
            mission_id: self.mission_id.clone(),
            iteration_id: self.iteration_id,
            workspace_root: self.workspace_root.clone(),
        };
        binding.validate(identity)?;
        if binding.correlation_id()? != self.correlation_id {
            return Err(EvidenceSpineError::corruption(format!(
                "event {} correlation id does not match mission identity",
                self.event_id
            )));
        }
        if digest_canonical(SOURCE_DIGEST_DOMAIN, &self.source)? != self.source_digest {
            return Err(EvidenceSpineError::corruption(format!(
                "event {} source projection digest mismatch",
                self.event_id
            )));
        }
        if self.compute_event_id()? != self.event_id {
            return Err(EvidenceSpineError::corruption(format!(
                "event {} id does not match its canonical bindings",
                self.event_id
            )));
        }
        validate_source_shape(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceSpineRowV1 {
    schema: String,
    sequence: u64,
    previous_row_digest: Option<String>,
    event: EvidenceSpineEventV1,
    row_digest: String,
}

impl EvidenceSpineRowV1 {
    fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_without_fields(ROW_DIGEST_DOMAIN, self, &["row_digest"])
    }

    fn seal(&mut self) -> Result<(), CanonicalError> {
        self.row_digest = self.compute_digest()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSpineRecoveryReportV1 {
    pub recovered_torn_tail_bytes: u64,
    /// Bytes after the last committed newline observed by a read-only open. They
    /// are excluded from the verified prefix but never truncated by a query.
    #[serde(default)]
    pub observed_uncommitted_tail_bytes: u64,
    pub verified_rows: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceAppendDisposition {
    Appended,
    Replayed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceAppendOutcomeV1 {
    pub disposition: EvidenceAppendDisposition,
    pub event_id: String,
    pub correlation_id: String,
    pub sequence: u64,
    pub row_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceCorrelationReadModelV1 {
    pub schema: String,
    pub correlation_id: String,
    pub organism_id: String,
    pub brain_id: String,
    pub mission_id: String,
    pub iteration_id: u64,
    pub workspace_root: String,
    pub receipt_id: Option<String>,
    pub receipt_digest: Option<String>,
    pub source_head_id: Option<String>,
    pub landed_head_id: Option<String>,
    pub transaction_id: Option<String>,
    pub delegation_ids: Vec<String>,
    pub mission_control_ids: Vec<String>,
    pub identity_digests: Vec<String>,
    pub evidence_digests: Vec<String>,
    pub event_ids: Vec<String>,
    pub latest_sequence: u64,
    pub landed_core_complete: bool,
    pub delegation_exactly_bound: bool,
    pub mission_control_exactly_bound: bool,
    pub cross_surface_complete: bool,
    pub gaps: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSpineQueryV1 {
    pub correlation_id: Option<String>,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
    pub transaction_id: Option<String>,
    pub receipt_id: Option<String>,
    pub delegation_id: Option<String>,
    pub mission_control_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSpineQueryResultV1 {
    pub schema: String,
    pub organism_id: String,
    pub brain_id: String,
    pub workspace_root: String,
    pub integrity: String,
    pub chain_head_digest: Option<String>,
    pub verified_rows: u64,
    pub recovered_torn_tail_bytes: u64,
    pub observed_uncommitted_tail_bytes: u64,
    pub correlations: Vec<EvidenceCorrelationReadModelV1>,
    pub non_claims: Vec<String>,
}

/// Owner-local, exclusive evidence projection. Holding the existing store lock
/// for the lifetime of this value preserves a single sequence/hash-chain writer.
pub struct EvidenceSpineStore {
    root: PathBuf,
    log_path: PathBuf,
    identity: EvidenceSpineIdentityV1,
    rows: Vec<EvidenceSpineRowV1>,
    recovery_report: EvidenceSpineRecoveryReportV1,
    _writer_lock: crate::light_author_handlers::LockGuard,
}

impl fmt::Debug for EvidenceSpineStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EvidenceSpineStore")
            .field("root", &self.root)
            .field("log_path", &self.log_path)
            .field("identity", &self.identity)
            .field("row_count", &self.rows.len())
            .field("recovery_report", &self.recovery_report)
            .finish_non_exhaustive()
    }
}

impl EvidenceSpineStore {
    pub fn open(
        root: impl AsRef<Path>,
        requested_identity: EvidenceSpineIdentityV1,
    ) -> EvidenceSpineResult<Self> {
        requested_identity.validate()?;
        let root = root.as_ref().to_path_buf();
        refuse_symlink_if_present(&root, "evidence spine root")?;
        std::fs::create_dir_all(&root)?;
        let writer_lock = crate::light_author_handlers::LockGuard::acquire_in(
            &root.join(".locks"),
            "evidence-spine-writer",
        )
        .map_err(|error| match error {
            m1nd_core::error::M1ndError::Io(error) => EvidenceSpineError::Io(error),
            other => EvidenceSpineError::refused("evidence_spine_lock_refused", other.to_string()),
        })?;

        let identity_path = root.join(EVIDENCE_SPINE_IDENTITY_FILE);
        let identity = if identity_path.exists() {
            refuse_symlink_if_present(&identity_path, "evidence spine identity")?;
            let identity: EvidenceSpineIdentityV1 =
                serde_json::from_slice(&read_file_no_follow(&identity_path)?)?;
            identity.validate()?;
            if identity != requested_identity {
                return Err(EvidenceSpineError::refused(
                    "evidence_spine_identity_mismatch",
                    "persisted organism/brain/workspace differs from the requested identity",
                ));
            }
            identity
        } else {
            let log_path = root.join(EVIDENCE_SPINE_LOG_FILE);
            if log_path.metadata().is_ok_and(|metadata| metadata.len() > 0) {
                return Err(EvidenceSpineError::corruption(
                    "correlation log exists without its binding identity",
                ));
            }
            write_json_atomic(&identity_path, &requested_identity)?;
            requested_identity
        };

        let log_path = root.join(EVIDENCE_SPINE_LOG_FILE);
        refuse_symlink_if_present(&log_path, "evidence spine log")?;
        let (rows, recovery_report) = load_rows(&log_path, &identity, true)?;
        build_read_models(&rows)?;
        Ok(Self {
            root,
            log_path,
            identity,
            rows,
            recovery_report,
            _writer_lock: writer_lock,
        })
    }

    /// Open an already-configured writer for the selected workspace. Identity is
    /// loaded from the owner-created identity file; callers cannot inject a brain
    /// id. The exact canonical workspace still has to match.
    pub fn open_existing_for_workspace(
        root: impl AsRef<Path>,
        expected_workspace_root: impl AsRef<Path>,
    ) -> EvidenceSpineResult<Self> {
        let root = root.as_ref();
        refuse_symlink_if_present(root, "evidence spine root")?;
        let identity = load_existing_identity(root)?;
        require_workspace_identity(&identity, expected_workspace_root.as_ref())?;
        Self::open(root, identity)
    }

    /// Verify and query an existing spine without creating a lock, directory,
    /// identity, or truncating a torn tail. This is the read-only REST/MCP seam.
    pub fn query_existing_read_only(
        root: impl AsRef<Path>,
        expected_workspace_root: impl AsRef<Path>,
        query: &EvidenceSpineQueryV1,
    ) -> EvidenceSpineResult<EvidenceSpineQueryResultV1> {
        let root = root.as_ref();
        refuse_symlink_if_present(root, "evidence spine root")?;
        let identity = load_existing_identity(root)?;
        require_workspace_identity(&identity, expected_workspace_root.as_ref())?;
        let log_path = root.join(EVIDENCE_SPINE_LOG_FILE);
        refuse_symlink_if_present(&log_path, "evidence spine log")?;
        let (rows, recovery_report) = load_rows(&log_path, &identity, false)?;
        query_rows(&identity, &rows, &recovery_report, query)
    }

    pub fn identity(&self) -> &EvidenceSpineIdentityV1 {
        &self.identity
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    pub fn recovery_report(&self) -> &EvidenceSpineRecoveryReportV1 {
        &self.recovery_report
    }

    pub fn record_receipt(
        &mut self,
        receipt: &ReceiptV1,
        observed_at: u64,
    ) -> EvidenceSpineResult<EvidenceAppendOutcomeV1> {
        validate_receipt(receipt)?;
        let binding = EvidenceMissionBindingV1::new(
            &self.identity,
            receipt.mission_id.clone(),
            receipt.iteration_id,
        )?;
        if receipt.brain_id != binding.brain_id {
            return Err(EvidenceSpineError::refused(
                "wrong_evidence_binding",
                "receipt brain does not match the opened evidence spine",
            ));
        }
        let evidence_digest = evidence_refs_digest(&receipt.evidence_refs)?;
        let identity_digest = digest_canonical(
            IDENTITY_SET_DIGEST_DOMAIN,
            &json!({
                "emitter": receipt.emitter,
                "issuer": receipt.issuer,
                "key_id": receipt.key_id,
                "algorithm": receipt.algorithm,
                "imported_by": receipt.import_audit.imported_by,
                "authority_snapshot_digest": receipt.import_audit.authority_snapshot_digest,
            }),
        )?;
        let source = EvidenceSpineSourceV1::Receipt {
            receipt_id: receipt.receipt_id.clone(),
            receipt_digest: receipt.receipt_digest.clone(),
            transaction_id: receipt.transaction_id.clone(),
            source_head_id: receipt.mission_head_id.clone(),
            candidate_digest: receipt.candidate_digest.clone(),
            block_id: receipt.scope.block_id.clone(),
            evidence_refs_digest: evidence_digest.clone(),
        };
        let event = EvidenceSpineEventV1::build(
            &binding,
            EvidenceCausalAttachmentV1 {
                mission_head_id: Some(receipt.mission_head_id.clone()),
                transaction_id: Some(receipt.transaction_id.clone()),
            },
            identity_digest,
            evidence_digest,
            source,
            observed_at,
        )?;
        self.append(event)
    }

    pub fn record_mission_letter(
        &mut self,
        letter: &MissionLetterV1,
        observed_at: u64,
    ) -> EvidenceSpineResult<EvidenceAppendOutcomeV1> {
        validate_letter(letter)?;
        let binding = EvidenceMissionBindingV1::new(
            &self.identity,
            letter.mission_id.clone(),
            letter.iteration_id,
        )?;
        if letter.brain_id != binding.brain_id {
            return Err(EvidenceSpineError::refused(
                "wrong_evidence_binding",
                "mission letter brain does not match the opened evidence spine",
            ));
        }
        let evidence_refs = letter
            .receipt_candidate
            .as_ref()
            .map(|candidate| candidate.evidence_refs.as_slice())
            .unwrap_or(&[]);
        let evidence_digest = evidence_refs_digest(evidence_refs)?;
        let identity_digest = digest_canonical(
            IDENTITY_SET_DIGEST_DOMAIN,
            &json!({
                "authored_by": letter.authored_by,
                "source": letter.source,
                "source_digest": letter.source_digest,
            }),
        )?;
        let source_head_id = if letter.state == MissionState::Landed {
            letter.previous_head_id.clone()
        } else {
            Some(letter.head_id.clone())
        };
        let source = EvidenceSpineSourceV1::MissionLetter {
            head_id: letter.head_id.clone(),
            previous_head_id: letter.previous_head_id.clone(),
            phase: letter.state,
            transaction_id: letter.transaction_id.clone(),
            packet_digest: letter.packet_digest.clone(),
            candidate_digest: letter
                .receipt_candidate
                .as_ref()
                .map(|candidate| candidate.candidate_digest.clone()),
            committed_receipt_id: letter.committed_receipt_id.clone(),
            evidence_refs_digest: evidence_digest.clone(),
        };
        let event = EvidenceSpineEventV1::build(
            &binding,
            EvidenceCausalAttachmentV1 {
                mission_head_id: source_head_id,
                transaction_id: letter.transaction_id.clone(),
            },
            identity_digest,
            evidence_digest,
            source,
            observed_at,
        )?;
        self.append(event)
    }

    pub fn record_delegation_packet(
        &mut self,
        binding: &EvidenceMissionBindingV1,
        attachment: EvidenceCausalAttachmentV1,
        packet: &Value,
        observed_at: u64,
    ) -> EvidenceSpineResult<EvidenceAppendOutcomeV1> {
        binding.validate(&self.identity)?;
        attachment.validate()?;
        let packet_object = packet.as_object().ok_or_else(|| {
            EvidenceSpineError::refused("invalid_delegation_packet", "packet is not an object")
        })?;
        if packet_object.get("schema").and_then(Value::as_str) != Some("m1nd-delegation-packet-v0")
        {
            return Err(EvidenceSpineError::refused(
                "unsupported_schema",
                "delegation packet must use m1nd-delegation-packet-v0",
            ));
        }
        let delegation_id = required_json_string(packet, &["delegation_id"])?;
        validate_prefixed_id("delegation_id", &delegation_id, "dlg_")?;
        let agent_id = required_json_string(packet, &["mission", "agent_id"])?;
        let packet_root = required_json_string(packet, &["mission", "binding", "workspace_root"])?;
        let packet_root = path_string(&canonical_existing_directory(Path::new(&packet_root))?)?;
        if packet_root != binding.workspace_root {
            return Err(EvidenceSpineError::refused(
                "wrong_workspace_binding",
                "delegation packet names another workspace root",
            ));
        }
        let mut canonical_packet = packet.clone();
        if let Value::Object(object) = &mut canonical_packet {
            object.remove("prompt_markdown");
            object.remove("status");
            object.remove("evidence_projection");
        }
        let packet_digest = digest_canonical(DELEGATION_PACKET_DIGEST_DOMAIN, &canonical_packet)?;
        let evidence_projection = json!({
            "staleness": packet.get("staleness"),
            "context": packet.get("context"),
            "known_static_dependents": packet.get("known_static_dependents"),
            "proof": packet.get("proof"),
        });
        let evidence_digest =
            digest_canonical(DELEGATION_EVIDENCE_DIGEST_DOMAIN, &evidence_projection)?;
        let identity_digest = digest_canonical(
            IDENTITY_SET_DIGEST_DOMAIN,
            &json!({"agent_id": agent_id, "workspace_root": packet_root}),
        )?;
        let source = EvidenceSpineSourceV1::DelegationPacket {
            delegation_id,
            packet_digest,
            agent_id,
            workspace_root: packet_root,
        };
        let event = EvidenceSpineEventV1::build(
            binding,
            attachment,
            identity_digest,
            evidence_digest,
            source,
            observed_at,
        )?;
        self.append(event)
    }

    /// Correlate the canonical debrief ledger row. This is intentionally a
    /// distinct source from the delegate packet so a debrief cannot collapse
    /// into the packet's idempotent replay.
    pub fn record_delegation_outcome(
        &mut self,
        binding: &EvidenceMissionBindingV1,
        attachment: EvidenceCausalAttachmentV1,
        outcome_record: &Value,
        observed_at: u64,
    ) -> EvidenceSpineResult<EvidenceAppendOutcomeV1> {
        binding.validate(&self.identity)?;
        attachment.validate()?;
        if outcome_record.get("schema").and_then(Value::as_str)
            != Some("m1nd-delegation-outcome-v0")
        {
            return Err(EvidenceSpineError::refused(
                "unsupported_schema",
                "delegation outcome must use m1nd-delegation-outcome-v0",
            ));
        }
        let delegation_id = required_json_string(outcome_record, &["delegation_id"])?;
        validate_prefixed_id("delegation_id", &delegation_id, "dlg_")?;
        let grader_id = required_json_string(outcome_record, &["grader"])?;
        let outcome = required_json_string(outcome_record, &["outcome"])?;
        if !matches!(outcome.as_str(), "success" | "failure" | "partial") {
            return Err(EvidenceSpineError::refused(
                "invalid_delegation_outcome",
                "delegation outcome must be success, failure, or partial",
            ));
        }
        let outcome_digest = digest_canonical(DELEGATION_PACKET_DIGEST_DOMAIN, outcome_record)?;
        let evidence_digest = digest_canonical(
            DELEGATION_EVIDENCE_DIGEST_DOMAIN,
            &json!({
                "outcome": outcome_record.get("outcome"),
                "outcome_unverified": outcome_record.get("outcome_unverified"),
                "graph_drifted": outcome_record.get("graph_drifted"),
                "touched_count": outcome_record.get("touched_count"),
                "unpredicted": outcome_record.get("unpredicted"),
            }),
        )?;
        let identity_digest = digest_canonical(
            IDENTITY_SET_DIGEST_DOMAIN,
            &json!({"grader_id": grader_id, "workspace_root": binding.workspace_root}),
        )?;
        let source = EvidenceSpineSourceV1::DelegationOutcome {
            delegation_id,
            outcome_digest,
            grader_id,
            outcome,
            workspace_root: binding.workspace_root.clone(),
        };
        let event = EvidenceSpineEventV1::build(
            binding,
            attachment,
            identity_digest,
            evidence_digest,
            source,
            observed_at,
        )?;
        self.append(event)
    }

    pub fn record_mission_control(
        &mut self,
        binding: &EvidenceMissionBindingV1,
        attachment: EvidenceCausalAttachmentV1,
        record: &Value,
        observed_at: u64,
    ) -> EvidenceSpineResult<EvidenceAppendOutcomeV1> {
        binding.validate(&self.identity)?;
        attachment.validate()?;
        if record.get("schema").and_then(Value::as_str) != Some("m1nd-mission-control-state-v1") {
            return Err(EvidenceSpineError::refused(
                "unsupported_schema",
                "Mission Control record must use m1nd-mission-control-state-v1",
            ));
        }
        let control_mission_id = required_json_string(record, &["mission_id"])?;
        validate_prefixed_id("mission_control_id", &control_mission_id, "msn_")?;
        let agent_id = required_json_string(record, &["agent_id"])?;
        let status = required_json_string(record, &["status"])?;
        let repo = required_json_string(record, &["repo"])?;
        let repo = path_string(&canonical_existing_directory(Path::new(&repo))?)?;
        if repo != binding.workspace_root {
            return Err(EvidenceSpineError::refused(
                "wrong_workspace_binding",
                "Mission Control record belongs to another repo",
            ));
        }
        let record_digest = digest_canonical(MISSION_CONTROL_RECORD_DIGEST_DOMAIN, record)?;
        let event_digest = digest_canonical(
            MISSION_CONTROL_EVIDENCE_DIGEST_DOMAIN,
            record.get("events").unwrap_or(&Value::Null),
        )?;
        let evidence_digest = digest_canonical(
            MISSION_CONTROL_EVIDENCE_DIGEST_DOMAIN,
            &json!({"events": record.get("events"), "claims": record.get("claims")}),
        )?;
        let identity_digest = digest_canonical(
            IDENTITY_SET_DIGEST_DOMAIN,
            &json!({"agent_id": agent_id, "repo": repo}),
        )?;
        let source = EvidenceSpineSourceV1::MissionControlRecord {
            control_mission_id,
            record_digest,
            event_digest,
            agent_id,
            status,
            repo,
        };
        let event = EvidenceSpineEventV1::build(
            binding,
            attachment,
            identity_digest,
            evidence_digest,
            source,
            observed_at,
        )?;
        self.append(event)
    }

    pub fn query(
        &self,
        query: &EvidenceSpineQueryV1,
    ) -> EvidenceSpineResult<EvidenceSpineQueryResultV1> {
        let mut result = query_rows(&self.identity, &self.rows, &self.recovery_report, query)?;
        // A writer-backed query has additionally passed exclusive open and all
        // append-time validation. Keep that stronger diagnostic distinct from
        // the read-only endpoint's verified-committed-prefix claim.
        result.integrity = "hash_chain_verified_on_open_and_append".to_string();
        Ok(result)
    }

    /// Require that a client-supplied correlation link is already anchored by a
    /// canonical G3 ReceiptV1 or MissionLetterV1 event. Coordination records can
    /// consume that link; they can never create its authority.
    pub fn validate_authority_anchor(
        &self,
        binding: &EvidenceMissionBindingV1,
        attachment: &EvidenceCausalAttachmentV1,
    ) -> EvidenceSpineResult<()> {
        binding.validate(&self.identity)?;
        attachment.validate()?;
        let correlation_id = binding.correlation_id()?;
        let anchored = self.rows.iter().any(|row| {
            row.event.correlation_id == correlation_id
                && row.event.mission_head_id == attachment.mission_head_id
                && attachment
                    .transaction_id
                    .as_ref()
                    .is_none_or(|transaction| {
                        row.event.transaction_id.as_ref() == Some(transaction)
                    })
                && matches!(
                    &row.event.source,
                    EvidenceSpineSourceV1::Receipt { .. }
                        | EvidenceSpineSourceV1::MissionLetter { .. }
                )
        });
        if !anchored {
            return Err(EvidenceSpineError::refused(
                "evidence_binding_unanchored",
                "correlation link does not match an existing canonical G3 receipt or mission-letter event",
            ));
        }
        Ok(())
    }

    fn append(
        &mut self,
        event: EvidenceSpineEventV1,
    ) -> EvidenceSpineResult<EvidenceAppendOutcomeV1> {
        event.validate(&self.identity)?;
        if let Some(existing) = self
            .rows
            .iter()
            .find(|row| row.event.event_id == event.event_id)
        {
            if !same_replay_event(&existing.event, &event) {
                return Err(EvidenceSpineError::corruption(format!(
                    "event id {} resolves to different bytes",
                    event.event_id
                )));
            }
            return Ok(EvidenceAppendOutcomeV1 {
                disposition: EvidenceAppendDisposition::Replayed,
                event_id: existing.event.event_id.clone(),
                correlation_id: existing.event.correlation_id.clone(),
                sequence: existing.sequence,
                row_digest: existing.row_digest.clone(),
            });
        }

        let sequence = self.rows.len() as u64 + 1;
        let mut row = EvidenceSpineRowV1 {
            schema: EVIDENCE_SPINE_ROW_SCHEMA.to_string(),
            sequence,
            previous_row_digest: self.rows.last().map(|row| row.row_digest.clone()),
            event,
            row_digest: String::new(),
        };
        row.seal()?;
        let mut candidate = self.rows.clone();
        candidate.push(row.clone());
        build_read_models(&candidate)?;

        append_row_durable(&self.log_path, &row)?;
        let outcome = EvidenceAppendOutcomeV1 {
            disposition: EvidenceAppendDisposition::Appended,
            event_id: row.event.event_id.clone(),
            correlation_id: row.event.correlation_id.clone(),
            sequence,
            row_digest: row.row_digest.clone(),
        };
        self.rows.push(row);
        self.recovery_report.verified_rows = self.rows.len() as u64;
        Ok(outcome)
    }
}

fn same_replay_event(left: &EvidenceSpineEventV1, right: &EvidenceSpineEventV1) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.observed_at = 0;
    right.observed_at = 0;
    left == right
}

fn validate_receipt(receipt: &ReceiptV1) -> EvidenceSpineResult<()> {
    require_schema("receipt", &receipt.schema, RECEIPT_SCHEMA)?;
    for (field, value) in [
        ("receipt.receipt_id", receipt.receipt_id.as_str()),
        ("receipt.transaction_id", receipt.transaction_id.as_str()),
        ("receipt.brain_id", receipt.brain_id.as_str()),
        ("receipt.mission_id", receipt.mission_id.as_str()),
        ("receipt.mission_head_id", receipt.mission_head_id.as_str()),
        (
            "receipt.candidate_digest",
            receipt.candidate_digest.as_str(),
        ),
    ] {
        require_non_empty(field, value)?;
    }
    if receipt.iteration_id == 0 || receipt.evidence_refs.is_empty() {
        return Err(EvidenceSpineError::refused(
            "invalid_receipt_binding",
            "receipt requires iteration >= 1 and at least one evidence ref",
        ));
    }
    require_digest("receipt.receipt_digest", &receipt.receipt_digest)?;
    require_digest("receipt.candidate_digest", &receipt.candidate_digest)?;
    let expected = receipt.compute_receipt_digest()?;
    if receipt.receipt_digest != expected || receipt.receipt_id != format!("rcp:{expected}") {
        return Err(EvidenceSpineError::refused(
            "receipt_digest_mismatch",
            "receipt id/digest differs from canonical ReceiptV1 bytes",
        ));
    }
    evidence_refs_digest(&receipt.evidence_refs)?;
    Ok(())
}

fn validate_letter(letter: &MissionLetterV1) -> EvidenceSpineResult<()> {
    require_schema("mission letter", &letter.schema, MISSION_LETTER_V1_SCHEMA)?;
    for (field, value) in [
        ("letter.head_id", letter.head_id.as_str()),
        ("letter.brain_id", letter.brain_id.as_str()),
        ("letter.mission_id", letter.mission_id.as_str()),
        ("letter.packet_digest", letter.packet_digest.as_str()),
        ("letter.authored_by", letter.authored_by.as_str()),
    ] {
        require_non_empty(field, value)?;
    }
    if letter.iteration_id == 0 {
        return Err(EvidenceSpineError::refused(
            "invalid_iteration",
            "mission letter requires iteration_id >= 1",
        ));
    }
    require_digest("letter.packet_digest", &letter.packet_digest)?;
    let expected = format!("mlt:{}", letter.compute_head_digest()?);
    if letter.head_id != expected {
        return Err(EvidenceSpineError::refused(
            "mission_letter_digest_mismatch",
            "mission letter head differs from canonical MissionLetterV1 bytes",
        ));
    }
    if let Some(candidate) = &letter.receipt_candidate {
        validate_candidate(candidate)?;
        if candidate.brain_id != letter.brain_id
            || candidate.mission_id != letter.mission_id
            || candidate.iteration_id != letter.iteration_id
        {
            return Err(EvidenceSpineError::refused(
                "candidate_binding_mismatch",
                "letter candidate does not bind the same brain/mission/iteration",
            ));
        }
    }
    if letter.state == MissionState::Landed
        && (letter.transaction_id.is_none()
            || letter.committed_receipt_id.is_none()
            || letter.previous_head_id.is_none()
            || letter.receipt_candidate.is_none())
    {
        return Err(EvidenceSpineError::refused(
            "landed_without_correlation",
            "landed letter requires previous head, transaction, candidate, and committed receipt",
        ));
    }
    Ok(())
}

fn validate_candidate(candidate: &ReceiptCandidateV1) -> EvidenceSpineResult<()> {
    require_schema(
        "receipt candidate",
        &candidate.schema,
        RECEIPT_CANDIDATE_SCHEMA,
    )?;
    if candidate.iteration_id == 0 || candidate.evidence_refs.is_empty() {
        return Err(EvidenceSpineError::refused(
            "invalid_candidate_binding",
            "candidate requires iteration >= 1 and evidence",
        ));
    }
    let expected = candidate.compute_candidate_digest()?;
    if candidate.candidate_digest != expected
        || candidate.candidate_id != format!("cand:{expected}")
    {
        return Err(EvidenceSpineError::refused(
            "candidate_digest_mismatch",
            "candidate id/digest differs from canonical ReceiptCandidateV1 bytes",
        ));
    }
    evidence_refs_digest(&candidate.evidence_refs)?;
    Ok(())
}

fn evidence_refs_digest(evidence_refs: &[EvidenceRefV1]) -> EvidenceSpineResult<String> {
    for evidence in evidence_refs {
        require_schema("evidence ref", &evidence.schema, EVIDENCE_REF_SCHEMA)?;
        require_digest("evidence.sha256", &evidence.sha256)?;
        require_digest("evidence.evidence_digest", &evidence.evidence_digest)?;
        if evidence.compute_evidence_digest()? != evidence.evidence_digest {
            return Err(EvidenceSpineError::refused(
                "evidence_digest_mismatch",
                "EvidenceRefV1 canonical digest mismatch",
            ));
        }
    }
    Ok(digest_canonical(
        EVIDENCE_SET_DIGEST_DOMAIN,
        &evidence_refs,
    )?)
}

fn validate_source_shape(event: &EvidenceSpineEventV1) -> EvidenceSpineResult<()> {
    match &event.source {
        EvidenceSpineSourceV1::Receipt {
            receipt_id,
            receipt_digest,
            transaction_id,
            source_head_id,
            candidate_digest,
            evidence_refs_digest,
            ..
        } => {
            validate_digest_id("receipt_id", receipt_id, "rcp:")?;
            require_digest("receipt_digest", receipt_digest)?;
            require_digest("candidate_digest", candidate_digest)?;
            require_digest("evidence_refs_digest", evidence_refs_digest)?;
            if event.mission_head_id.as_deref() != Some(source_head_id)
                || event.transaction_id.as_deref() != Some(transaction_id)
                || event.evidence_digest != *evidence_refs_digest
            {
                return Err(EvidenceSpineError::corruption(
                    "receipt projection disagrees with the event causal bindings",
                ));
            }
        }
        EvidenceSpineSourceV1::MissionLetter {
            head_id,
            previous_head_id,
            phase,
            transaction_id,
            candidate_digest,
            committed_receipt_id,
            evidence_refs_digest,
            ..
        } => {
            validate_digest_id("head_id", head_id, "mlt:")?;
            require_optional_digest("candidate_digest", candidate_digest.as_deref())?;
            if let Some(receipt_id) = committed_receipt_id {
                validate_digest_id("committed_receipt_id", receipt_id, "rcp:")?;
            }
            require_digest("evidence_refs_digest", evidence_refs_digest)?;
            let expected_head = if *phase == MissionState::Landed {
                previous_head_id.as_ref()
            } else {
                Some(head_id)
            };
            if event.mission_head_id.as_ref() != expected_head
                || event.transaction_id != *transaction_id
                || event.evidence_digest != *evidence_refs_digest
            {
                return Err(EvidenceSpineError::corruption(
                    "mission-letter projection disagrees with event causal bindings",
                ));
            }
        }
        EvidenceSpineSourceV1::DelegationPacket {
            delegation_id,
            packet_digest,
            agent_id,
            workspace_root,
        } => {
            validate_prefixed_id("delegation_id", delegation_id, "dlg_")?;
            require_digest("packet_digest", packet_digest)?;
            require_non_empty("delegation.agent_id", agent_id)?;
            if workspace_root != &event.workspace_root {
                return Err(EvidenceSpineError::corruption(
                    "delegation projection workspace differs from event binding",
                ));
            }
        }
        EvidenceSpineSourceV1::DelegationOutcome {
            delegation_id,
            outcome_digest,
            grader_id,
            outcome,
            workspace_root,
        } => {
            validate_prefixed_id("delegation_id", delegation_id, "dlg_")?;
            require_digest("outcome_digest", outcome_digest)?;
            require_non_empty("delegation.grader_id", grader_id)?;
            if !matches!(outcome.as_str(), "success" | "failure" | "partial") {
                return Err(EvidenceSpineError::corruption(
                    "delegation outcome projection has an invalid outcome",
                ));
            }
            if workspace_root != &event.workspace_root {
                return Err(EvidenceSpineError::corruption(
                    "delegation outcome workspace differs from event binding",
                ));
            }
        }
        EvidenceSpineSourceV1::MissionControlRecord {
            control_mission_id,
            record_digest,
            event_digest,
            agent_id,
            status,
            repo,
        } => {
            validate_prefixed_id("mission_control_id", control_mission_id, "msn_")?;
            require_digest("record_digest", record_digest)?;
            require_digest("event_digest", event_digest)?;
            require_non_empty("mission_control.agent_id", agent_id)?;
            require_non_empty("mission_control.status", status)?;
            if repo != &event.workspace_root {
                return Err(EvidenceSpineError::corruption(
                    "Mission Control projection repo differs from event binding",
                ));
            }
        }
    }
    Ok(())
}

fn build_read_models(
    rows: &[EvidenceSpineRowV1],
) -> EvidenceSpineResult<Vec<EvidenceCorrelationReadModelV1>> {
    let mut grouped: BTreeMap<String, Vec<&EvidenceSpineRowV1>> = BTreeMap::new();
    for row in rows {
        grouped
            .entry(row.event.correlation_id.clone())
            .or_default()
            .push(row);
    }

    let mut models = Vec::with_capacity(grouped.len());
    for (correlation_id, correlation_rows) in grouped {
        let first = correlation_rows[0];
        let mut receipts = Vec::new();
        let mut landed_letters = Vec::new();
        let mut delegation_ids = BTreeSet::new();
        let mut mission_control_ids = BTreeSet::new();
        let mut identity_digests = BTreeSet::new();
        let mut evidence_digests = BTreeSet::new();
        let mut event_ids = Vec::new();
        let mut delegation_events = Vec::new();
        let mut mission_control_events = Vec::new();

        for row in &correlation_rows {
            let event = &row.event;
            if event.organism_id != first.event.organism_id
                || event.brain_id != first.event.brain_id
                || event.mission_id != first.event.mission_id
                || event.iteration_id != first.event.iteration_id
                || event.workspace_root != first.event.workspace_root
            {
                return Err(EvidenceSpineError::corruption(format!(
                    "correlation {correlation_id} mixes incompatible mission bindings"
                )));
            }
            identity_digests.insert(event.identity_digest.clone());
            evidence_digests.insert(event.evidence_digest.clone());
            event_ids.push(event.event_id.clone());
            match &event.source {
                EvidenceSpineSourceV1::Receipt { .. } => receipts.push(*row),
                EvidenceSpineSourceV1::MissionLetter { phase, .. }
                    if *phase == MissionState::Landed =>
                {
                    landed_letters.push(*row)
                }
                EvidenceSpineSourceV1::DelegationPacket { delegation_id, .. } => {
                    delegation_ids.insert(delegation_id.clone());
                    delegation_events.push(*row);
                }
                EvidenceSpineSourceV1::DelegationOutcome { delegation_id, .. } => {
                    delegation_ids.insert(delegation_id.clone());
                    delegation_events.push(*row);
                }
                EvidenceSpineSourceV1::MissionControlRecord {
                    control_mission_id, ..
                } => {
                    mission_control_ids.insert(control_mission_id.clone());
                    mission_control_events.push(*row);
                }
                EvidenceSpineSourceV1::MissionLetter { .. } => {}
            }
        }

        let receipt = unique_source(&receipts, "receipt", &correlation_id)?;
        let landed = unique_source(&landed_letters, "landed letter", &correlation_id)?;
        let mut gaps = Vec::new();
        let mut receipt_id = None;
        let mut receipt_digest = None;
        let mut source_head_id = None;
        let mut landed_head_id = None;
        let mut transaction_id = None;
        let mut landed_core_complete = false;

        if let Some(receipt_row) = receipt {
            if let EvidenceSpineSourceV1::Receipt {
                receipt_id: id,
                receipt_digest: digest,
                transaction_id: transaction,
                source_head_id: head,
                ..
            } = &receipt_row.event.source
            {
                receipt_id = Some(id.clone());
                receipt_digest = Some(digest.clone());
                source_head_id = Some(head.clone());
                transaction_id = Some(transaction.clone());
            }
        } else {
            gaps.push("receipt_missing".to_string());
        }

        if let Some(landed_row) = landed {
            if let EvidenceSpineSourceV1::MissionLetter { head_id, .. } = &landed_row.event.source {
                landed_head_id = Some(head_id.clone());
            }
        } else {
            gaps.push("landed_letter_missing".to_string());
        }

        if let (Some(receipt_row), Some(landed_row)) = (receipt, landed) {
            validate_landed_join(receipt_row, landed_row, &correlation_id)?;
            landed_core_complete = true;
        }

        let exact = |row: &&EvidenceSpineRowV1| {
            landed_core_complete
                && row.event.mission_head_id == source_head_id
                && row.event.transaction_id == transaction_id
        };
        let delegation_exactly_bound = delegation_events.iter().any(exact);
        let mission_control_exactly_bound = mission_control_events.iter().any(exact);
        if delegation_ids.is_empty() {
            gaps.push("delegation_packet_missing".to_string());
        } else if !delegation_exactly_bound {
            gaps.push("delegation_exact_head_transaction_binding_missing".to_string());
        }
        if mission_control_ids.is_empty() {
            gaps.push("mission_control_record_missing".to_string());
        } else if !mission_control_exactly_bound {
            gaps.push("mission_control_exact_head_transaction_binding_missing".to_string());
        }
        let cross_surface_complete =
            landed_core_complete && delegation_exactly_bound && mission_control_exactly_bound;

        models.push(EvidenceCorrelationReadModelV1 {
            schema: EVIDENCE_CORRELATION_READ_MODEL_SCHEMA.to_string(),
            correlation_id,
            organism_id: first.event.organism_id.clone(),
            brain_id: first.event.brain_id.clone(),
            mission_id: first.event.mission_id.clone(),
            iteration_id: first.event.iteration_id,
            workspace_root: first.event.workspace_root.clone(),
            receipt_id,
            receipt_digest,
            source_head_id,
            landed_head_id,
            transaction_id,
            delegation_ids: delegation_ids.into_iter().collect(),
            mission_control_ids: mission_control_ids.into_iter().collect(),
            identity_digests: identity_digests.into_iter().collect(),
            evidence_digests: evidence_digests.into_iter().collect(),
            event_ids,
            latest_sequence: correlation_rows
                .last()
                .map(|row| row.sequence)
                .unwrap_or_default(),
            landed_core_complete,
            delegation_exactly_bound,
            mission_control_exactly_bound,
            cross_surface_complete,
            gaps,
        });
    }
    Ok(models)
}

fn unique_source<'a>(
    rows: &'a [&'a EvidenceSpineRowV1],
    label: &str,
    correlation_id: &str,
) -> EvidenceSpineResult<Option<&'a EvidenceSpineRowV1>> {
    let unique_ids: BTreeSet<&str> = rows.iter().map(|row| row.event.source.key()).collect();
    if unique_ids.len() > 1 {
        return Err(EvidenceSpineError::refused(
            "correlation_conflict",
            format!("correlation {correlation_id} contains more than one {label}"),
        ));
    }
    Ok(rows.last().copied())
}

fn validate_landed_join(
    receipt_row: &EvidenceSpineRowV1,
    landed_row: &EvidenceSpineRowV1,
    correlation_id: &str,
) -> EvidenceSpineResult<()> {
    let EvidenceSpineSourceV1::Receipt {
        receipt_id,
        transaction_id,
        source_head_id,
        candidate_digest,
        evidence_refs_digest,
        ..
    } = &receipt_row.event.source
    else {
        unreachable!("caller selected receipt source")
    };
    let EvidenceSpineSourceV1::MissionLetter {
        previous_head_id,
        transaction_id: letter_transaction,
        candidate_digest: letter_candidate,
        committed_receipt_id,
        evidence_refs_digest: letter_evidence,
        ..
    } = &landed_row.event.source
    else {
        unreachable!("caller selected landed-letter source")
    };

    if previous_head_id.as_deref() != Some(source_head_id)
        || letter_transaction.as_deref() != Some(transaction_id)
        || committed_receipt_id.as_deref() != Some(receipt_id)
        || letter_candidate.as_deref() != Some(candidate_digest)
        || letter_evidence != evidence_refs_digest
    {
        return Err(EvidenceSpineError::refused(
            "landed_correlation_mismatch",
            format!(
                "correlation {correlation_id} receipt and landed letter disagree on head/transaction/receipt/candidate/evidence"
            ),
        ));
    }
    Ok(())
}

fn query_matches(query: &EvidenceSpineQueryV1, model: &EvidenceCorrelationReadModelV1) -> bool {
    query
        .correlation_id
        .as_ref()
        .is_none_or(|value| value == &model.correlation_id)
        && query
            .mission_id
            .as_ref()
            .is_none_or(|value| value == &model.mission_id)
        && query
            .transaction_id
            .as_ref()
            .is_none_or(|value| model.transaction_id.as_ref() == Some(value))
        && query
            .receipt_id
            .as_ref()
            .is_none_or(|value| model.receipt_id.as_ref() == Some(value))
        && query
            .delegation_id
            .as_ref()
            .is_none_or(|value| model.delegation_ids.contains(value))
        && query
            .mission_control_id
            .as_ref()
            .is_none_or(|value| model.mission_control_ids.contains(value))
}

fn query_rows(
    identity: &EvidenceSpineIdentityV1,
    rows: &[EvidenceSpineRowV1],
    recovery_report: &EvidenceSpineRecoveryReportV1,
    query: &EvidenceSpineQueryV1,
) -> EvidenceSpineResult<EvidenceSpineQueryResultV1> {
    let mut correlations = build_read_models(rows)?;
    correlations.retain(|model| {
        query_matches(query, model)
            && query.mission_head_id.as_ref().is_none_or(|expected| {
                rows.iter().any(|row| {
                    row.event.correlation_id == model.correlation_id
                        && (row.event.mission_head_id.as_ref() == Some(expected)
                            || matches!(
                                &row.event.source,
                                EvidenceSpineSourceV1::MissionLetter { head_id, .. }
                                    if head_id == expected
                            ))
                })
            })
    });
    let integrity = if recovery_report.observed_uncommitted_tail_bytes == 0 {
        "hash_chain_verified_committed_rows".to_string()
    } else {
        "hash_chain_verified_committed_prefix_uncommitted_tail_observed".to_string()
    };
    Ok(EvidenceSpineQueryResultV1 {
        schema: EVIDENCE_SPINE_QUERY_RESULT_SCHEMA.to_string(),
        organism_id: identity.organism_id.clone(),
        brain_id: identity.brain_id.clone(),
        workspace_root: identity.workspace_root.clone(),
        integrity,
        chain_head_digest: rows.last().map(|row| row.row_digest.clone()),
        verified_rows: rows.len() as u64,
        recovered_torn_tail_bytes: recovery_report.recovered_torn_tail_bytes,
        observed_uncommitted_tail_bytes: recovery_report.observed_uncommitted_tail_bytes,
        correlations,
        non_claims: vec![
            "the evidence spine is a read projection; it is not receipt, mission-state, delegation, or Mission Control authority".to_string(),
            "digest correlation does not prove external evidence bytes still exist or remain fresh".to_string(),
            "hash-chain integrity is local-store integrity, not production key attestation".to_string(),
            "a read-only query reports but never truncates an uncommitted torn tail".to_string(),
        ],
    })
}

fn load_rows(
    path: &Path,
    identity: &EvidenceSpineIdentityV1,
    repair_torn_tail: bool,
) -> EvidenceSpineResult<(Vec<EvidenceSpineRowV1>, EvidenceSpineRecoveryReportV1)> {
    if !path.exists() {
        return Ok((Vec::new(), EvidenceSpineRecoveryReportV1::default()));
    }
    refuse_symlink_if_present(path, "evidence spine log")?;
    let mut bytes = read_file_no_follow(path)?;
    let mut recovered_torn_tail_bytes = 0;
    let mut observed_uncommitted_tail_bytes = 0;
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        let committed_len = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        observed_uncommitted_tail_bytes = (bytes.len() - committed_len) as u64;
        if repair_torn_tail {
            recovered_torn_tail_bytes = observed_uncommitted_tail_bytes;
            let file = open_write_no_follow(path)?;
            file.set_len(committed_len as u64)?;
            file.sync_all()?;
            #[cfg(unix)]
            sync_parent_directory(path.parent().unwrap_or_else(|| Path::new(".")))?;
            observed_uncommitted_tail_bytes = 0;
        }
        bytes.truncate(committed_len);
    }

    let mut rows = Vec::new();
    for (index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        let row: EvidenceSpineRowV1 = serde_json::from_slice(line).map_err(|error| {
            EvidenceSpineError::corruption(format!(
                "complete log row {} is not valid JSON: {error}",
                index + 1
            ))
        })?;
        require_schema("evidence log row", &row.schema, EVIDENCE_SPINE_ROW_SCHEMA)?;
        let expected_sequence = rows.len() as u64 + 1;
        if row.sequence != expected_sequence {
            return Err(EvidenceSpineError::corruption(format!(
                "row sequence mismatch: expected {expected_sequence}, observed {}",
                row.sequence
            )));
        }
        let expected_previous = rows
            .last()
            .map(|previous: &EvidenceSpineRowV1| previous.row_digest.clone());
        if row.previous_row_digest != expected_previous {
            return Err(EvidenceSpineError::corruption(format!(
                "row {} previous digest does not match chain head",
                row.sequence
            )));
        }
        require_digest("row.row_digest", &row.row_digest)?;
        if row.compute_digest()? != row.row_digest {
            return Err(EvidenceSpineError::corruption(format!(
                "row {} digest mismatch",
                row.sequence
            )));
        }
        row.event.validate(identity)?;
        rows.push(row);
    }
    let report = EvidenceSpineRecoveryReportV1 {
        recovered_torn_tail_bytes,
        observed_uncommitted_tail_bytes,
        verified_rows: rows.len() as u64,
    };
    Ok((rows, report))
}

fn append_row_durable(path: &Path, row: &EvidenceSpineRowV1) -> EvidenceSpineResult<()> {
    let existed = path.exists();
    refuse_symlink_if_present(path, "evidence spine log")?;
    let mut bytes = serde_json::to_vec(row)?;
    bytes.push(b'\n');
    let mut file = open_append_no_follow(path)?;
    file.write_all(&bytes)?;
    file.flush()?;
    file.sync_all()?;
    if !existed {
        #[cfg(unix)]
        sync_parent_directory(path.parent().unwrap_or_else(|| Path::new(".")))?;
    }
    Ok(())
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> EvidenceSpineResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    refuse_symlink_if_present(path, "evidence spine atomic target")?;
    let bytes = serde_json::to_vec_pretty(value)?;
    let (temporary, mut file) = (0..128)
        .find_map(|_| {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let temporary = parent.join(format!(
                ".{}.{}.{}.tmp",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("evidence-spine"),
                std::process::id(),
                sequence
            ));
            match open_create_new_no_follow(&temporary) {
                Ok(file) => Some(Ok((temporary, file))),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .transpose()?
        .ok_or_else(|| {
            EvidenceSpineError::Io(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not allocate a unique evidence-spine temporary file",
            ))
        })?;
    let result = (|| -> io::Result<()> {
        file.write_all(&bytes)?;
        file.flush()?;
        file.sync_all()?;
        // Windows publication cannot rename a source handle that denied
        // FILE_SHARE_DELETE; close the no-follow temporary before MoveFileExW.
        drop(file);
        replace_atomic_write_through(&temporary, path, parent)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(EvidenceSpineError::Io)
}

fn load_existing_identity(root: &Path) -> EvidenceSpineResult<EvidenceSpineIdentityV1> {
    if !root.is_dir() {
        return Err(EvidenceSpineError::refused(
            "evidence_spine_not_configured",
            format!("evidence spine root '{}' does not exist", root.display()),
        ));
    }
    let identity_path = root.join(EVIDENCE_SPINE_IDENTITY_FILE);
    refuse_symlink_if_present(&identity_path, "evidence spine identity")?;
    if !identity_path.is_file() {
        return Err(EvidenceSpineError::refused(
            "evidence_spine_not_configured",
            "owner has not installed an evidence-spine identity for this brain",
        ));
    }
    let identity: EvidenceSpineIdentityV1 =
        serde_json::from_slice(&read_file_no_follow(&identity_path)?)?;
    identity.validate()?;
    Ok(identity)
}

fn require_workspace_identity(
    identity: &EvidenceSpineIdentityV1,
    expected_workspace_root: &Path,
) -> EvidenceSpineResult<()> {
    let expected = canonical_existing_directory(expected_workspace_root)?;
    if identity.workspace_root != path_string(&expected)? {
        return Err(EvidenceSpineError::refused(
            "wrong_workspace_binding",
            "selected owner brain does not match the evidence-spine workspace identity",
        ));
    }
    Ok(())
}

fn canonical_existing_directory(path: &Path) -> EvidenceSpineResult<PathBuf> {
    if !path.is_absolute() {
        return Err(EvidenceSpineError::refused(
            "invalid_workspace_root",
            "workspace root must be absolute",
        ));
    }
    let canonical = std::fs::canonicalize(path).map_err(|error| {
        EvidenceSpineError::refused(
            "invalid_workspace_root",
            format!(
                "workspace root '{}' cannot be canonicalized: {error}",
                path.display()
            ),
        )
    })?;
    if !canonical.is_dir() {
        return Err(EvidenceSpineError::refused(
            "invalid_workspace_root",
            format!(
                "workspace root '{}' is not a directory",
                canonical.display()
            ),
        ));
    }
    Ok(canonical)
}

fn path_string(path: &Path) -> EvidenceSpineResult<String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| EvidenceSpineError::refused("non_utf8_path", path.display().to_string()))
}

fn refuse_symlink_if_present(path: &Path, label: &'static str) -> EvidenceSpineResult<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata_is_link_or_reparse(&metadata) => Err(EvidenceSpineError::refused(
            "symlink_refused",
            format!(
                "{label} '{}' must not be a symlink or Windows reparse point",
                path.display()
            ),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(EvidenceSpineError::Io(error)),
    }
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

fn required_json_string(value: &Value, path: &[&str]) -> EvidenceSpineResult<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key).ok_or_else(|| {
            EvidenceSpineError::refused(
                "missing_correlation_field",
                format!("missing JSON field {}", path.join(".")),
            )
        })?;
    }
    let value = current.as_str().ok_or_else(|| {
        EvidenceSpineError::refused(
            "invalid_correlation_field",
            format!("JSON field {} must be a string", path.join(".")),
        )
    })?;
    require_non_empty("JSON correlation field", value)?;
    Ok(value.to_string())
}

fn validate_prefixed_id(
    field: &'static str,
    value: &str,
    prefix: &'static str,
) -> EvidenceSpineResult<()> {
    if value.starts_with(prefix)
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | ':')
        })
    {
        Ok(())
    } else {
        Err(EvidenceSpineError::refused(
            "invalid_correlation_id",
            format!("{field} must start with {prefix} and contain no path separators"),
        ))
    }
}

fn validate_digest_id(
    field: &'static str,
    value: &str,
    prefix: &'static str,
) -> EvidenceSpineResult<()> {
    let digest = value.strip_prefix(prefix).ok_or_else(|| {
        EvidenceSpineError::refused(
            "invalid_digest_id",
            format!("{field} must start with {prefix}"),
        )
    })?;
    require_digest(field, digest)
}

fn require_schema(
    contract: &'static str,
    actual: &str,
    expected: &'static str,
) -> EvidenceSpineResult<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(EvidenceSpineError::refused(
            "unsupported_schema",
            format!("{contract}: expected '{expected}', observed '{actual}'"),
        ))
    }
}

fn require_non_empty(field: &'static str, value: &str) -> EvidenceSpineResult<()> {
    if value.trim().is_empty() {
        Err(EvidenceSpineError::refused("empty_required_field", field))
    } else {
        Ok(())
    }
}

fn require_optional_non_empty(field: &'static str, value: Option<&str>) -> EvidenceSpineResult<()> {
    match value {
        Some(value) => require_non_empty(field, value),
        None => Ok(()),
    }
}

fn require_digest(field: &'static str, value: &str) -> EvidenceSpineResult<()> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(EvidenceSpineError::refused(
            "invalid_digest",
            format!("{field} is not a lowercase SHA-256 digest"),
        ))
    }
}

fn require_optional_digest(field: &'static str, value: Option<&str>) -> EvidenceSpineResult<()> {
    match value {
        Some(value) => require_digest(field, value),
        None => Ok(()),
    }
}

fn digest_without_fields<T: Serialize>(
    domain: &str,
    value: &T,
    fields: &[&str],
) -> Result<String, CanonicalError> {
    let mut value = serde_json::to_value(value)?;
    if let Value::Object(object) = &mut value {
        for field in fields {
            object.remove(*field);
        }
    }
    digest_canonical(domain, &value)
}

fn read_file_no_follow(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = open_read_no_follow(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(unix)]
fn open_read_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

#[cfg(windows)]
fn open_read_no_follow(path: &Path) -> io::Result<File> {
    crate::windows_durable_fs::open_read_no_follow(path)
}

#[cfg(all(not(unix), not(windows)))]
fn open_read_no_follow(_path: &Path) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "evidence spine no-follow reads are not proven on this platform",
    ))
}

#[cfg(unix)]
fn open_write_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

#[cfg(windows)]
fn open_write_no_follow(path: &Path) -> io::Result<File> {
    crate::windows_durable_fs::open_write_no_follow(path)
}

#[cfg(all(not(unix), not(windows)))]
fn open_write_no_follow(_path: &Path) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "evidence spine no-follow writes are not proven on this platform",
    ))
}

#[cfg(unix)]
fn open_append_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .create(true)
        .append(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

#[cfg(windows)]
fn open_append_no_follow(path: &Path) -> io::Result<File> {
    crate::windows_durable_fs::open_read_append_create_no_follow(path)
}

#[cfg(all(not(unix), not(windows)))]
fn open_append_no_follow(_path: &Path) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "evidence spine durable append is not proven on this platform",
    ))
}

#[cfg(unix)]
fn open_create_new_no_follow(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

#[cfg(windows)]
fn open_create_new_no_follow(path: &Path) -> io::Result<File> {
    crate::windows_durable_fs::open_create_new_no_follow(path)
}

#[cfg(all(not(unix), not(windows)))]
fn open_create_new_no_follow(_path: &Path) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "evidence spine atomic creation is not proven on this platform",
    ))
}

#[cfg(unix)]
fn replace_atomic_write_through(
    source: &Path,
    destination: &Path,
    parent: &Path,
) -> io::Result<()> {
    std::fs::rename(source, destination)?;
    sync_parent_directory(parent)
}

#[cfg(windows)]
fn replace_atomic_write_through(
    source: &Path,
    destination: &Path,
    _parent: &Path,
) -> io::Result<()> {
    crate::windows_durable_fs::replace_write_through(source, destination)
}

#[cfg(all(not(unix), not(windows)))]
fn replace_atomic_write_through(
    _source: &Path,
    _destination: &Path,
    _parent: &Path,
) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "evidence spine atomic replacement is not proven on this platform",
    ))
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> io::Result<()> {
    File::open(parent)?.sync_all()
}
