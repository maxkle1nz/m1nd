use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{digest_canonical, ActiveMode, CanonicalError, OpaqueSignature};

pub const RELEASE_CANDIDATE_SCHEMA: &str = "m1nd-release-candidate-manifest-v1";
pub const RELEASE_CANDIDATE_DIGEST_DOMAIN: &str = "m1nd-release-candidate-manifest-v1";
pub const GATE_RECEIPT_SCHEMA: &str = "m1nd-gate-receipt-v1";
pub const GATE_RECEIPT_DIGEST_DOMAIN: &str = "m1nd-gate-receipt-v1";
pub const INDEPENDENT_REVIEW_RECEIPT_SCHEMA: &str =
    "m1nd-independent-adversarial-review-receipt-v1";
pub const INDEPENDENT_REVIEW_RECEIPT_DIGEST_DOMAIN: &str =
    "m1nd-independent-adversarial-review-receipt-v1";
pub const METRIC_SPEC_SCHEMA: &str = "m1nd-metric-spec-v1";
pub const METRIC_SPEC_DIGEST_DOMAIN: &str = "m1nd-metric-spec-v1";

/// The custody-floor identifier ratified for the current M1ND-10 activation era
/// (amendment G9-A1; `docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md` §5). It names
/// the floor the *authority custody era* stands on, not a property of any
/// candidate. `m1nd-mcp::enclave_authority` re-exports this constant so the
/// custody-ceremony receipt and the gate/autonomy receipts name one literal.
pub const SECURE_ENCLAVE_CUSTODY_FLOOR_V1: &str = "secure-enclave-single-host-v1";

/// The closed set of custody floors a receipt minted in this era may declare.
/// Today it holds exactly the ratified Secure Enclave single-host floor; a
/// successor Path-A era will carry its own value here. Any other value is refused
/// at the structural gate, so no receipt can name a floor the program never
/// ratified. The production value is drawn from this constant / the ceremony
/// receipt, never from request payload.
pub const RATIFIED_CUSTODY_FLOORS: &[&str] = &[SECURE_ENCLAVE_CUSTODY_FLOOR_V1];

/// Whether `floor` is a member of the closed [`RATIFIED_CUSTODY_FLOORS`] set.
pub fn is_ratified_custody_floor(floor: &str) -> bool {
    RATIFIED_CUSTODY_FLOORS.contains(&floor)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GateId {
    G0,
    G1,
    G2,
    G3,
    G4,
    G5,
    G6,
    G7,
    G8,
    G9,
    G10,
}

impl GateId {
    pub const ALL: [Self; 11] = [
        Self::G0,
        Self::G1,
        Self::G2,
        Self::G3,
        Self::G4,
        Self::G5,
        Self::G6,
        Self::G7,
        Self::G8,
        Self::G9,
        Self::G10,
    ];
}

impl fmt::Display for GateId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GateVerdict {
    Pass,
    Fail,
    NotRun,
    NotProven,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingSeverity {
    P0,
    P1,
    P2,
    P3,
    Info,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FindingStatus {
    Open,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseFindingV1 {
    pub finding_id: String,
    pub severity: FindingSeverity,
    pub status: FindingStatus,
    pub statement: String,
    pub evidence_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseCandidateManifestCoreV1 {
    pub repo_commits: BTreeMap<String, String>,
    pub artifact_digests: BTreeMap<String, String>,
    pub schema_policy_versions: BTreeMap<String, String>,
    pub tool_catalog_digest: String,
    pub safety_kernel_digest: String,
    pub previous_governance_runtime_digest: String,
    pub constitution_epoch_digest: String,
    pub autonomy_epoch_grants_digest: String,
    pub independence_quorum_policy_digest: String,
    pub intended_active_mode: ActiveMode,
    pub compatibility_manifest_digest: String,
    pub rollback_plan_digest: String,
    pub harness_fixture_threat_digests: BTreeMap<String, String>,
    pub build_environment_digest: String,
    pub built_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseCandidateManifestV1 {
    pub schema: String,
    pub core: ReleaseCandidateManifestCoreV1,
    pub candidate_digest: String,
    pub provenance_signature: OpaqueSignature,
}

impl ReleaseCandidateManifestV1 {
    pub fn seal(
        core: ReleaseCandidateManifestCoreV1,
        provenance_signature: OpaqueSignature,
    ) -> Result<Self, ReleaseContractError> {
        let candidate_digest = digest_canonical(RELEASE_CANDIDATE_DIGEST_DOMAIN, &core)?;
        let candidate = Self {
            schema: RELEASE_CANDIDATE_SCHEMA.to_string(),
            core,
            candidate_digest,
            provenance_signature,
        };
        candidate.validate()?;
        Ok(candidate)
    }

    pub fn validate(&self) -> Result<ReleaseStructuralValidation, ReleaseContractError> {
        require_schema("release candidate", &self.schema, RELEASE_CANDIDATE_SCHEMA)?;
        require_non_empty_map("repo_commits", &self.core.repo_commits)?;
        require_non_empty_map("artifact_digests", &self.core.artifact_digests)?;
        require_non_empty_map("schema_policy_versions", &self.core.schema_policy_versions)?;
        require_non_empty_map(
            "harness_fixture_threat_digests",
            &self.core.harness_fixture_threat_digests,
        )?;
        validate_named_values("repo_commits", &self.core.repo_commits, false)?;
        validate_named_values("artifact_digests", &self.core.artifact_digests, true)?;
        validate_named_values(
            "schema_policy_versions",
            &self.core.schema_policy_versions,
            false,
        )?;
        validate_named_values(
            "harness_fixture_threat_digests",
            &self.core.harness_fixture_threat_digests,
            true,
        )?;
        for (field, digest) in [
            ("candidate_digest", self.candidate_digest.as_str()),
            (
                "tool_catalog_digest",
                self.core.tool_catalog_digest.as_str(),
            ),
            (
                "safety_kernel_digest",
                self.core.safety_kernel_digest.as_str(),
            ),
            (
                "previous_governance_runtime_digest",
                self.core.previous_governance_runtime_digest.as_str(),
            ),
            (
                "constitution_epoch_digest",
                self.core.constitution_epoch_digest.as_str(),
            ),
            (
                "autonomy_epoch_grants_digest",
                self.core.autonomy_epoch_grants_digest.as_str(),
            ),
            (
                "independence_quorum_policy_digest",
                self.core.independence_quorum_policy_digest.as_str(),
            ),
            (
                "compatibility_manifest_digest",
                self.core.compatibility_manifest_digest.as_str(),
            ),
            (
                "rollback_plan_digest",
                self.core.rollback_plan_digest.as_str(),
            ),
            (
                "build_environment_digest",
                self.core.build_environment_digest.as_str(),
            ),
        ] {
            require_digest(field, digest)?;
        }
        require_signature("provenance_signature", &self.provenance_signature)?;
        let computed = digest_canonical(RELEASE_CANDIDATE_DIGEST_DOMAIN, &self.core)?;
        if computed != self.candidate_digest {
            return Err(ReleaseContractError::DigestMismatch {
                field: "candidate_digest",
            });
        }
        Ok(ReleaseStructuralValidation::not_authenticated())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateReceiptCoreV1 {
    pub candidate_digest: String,
    pub gate_id: GateId,
    /// Custody floor of the authority custody era under which this receipt was
    /// minted (era-scoped; a successor Path-A era will carry a different value).
    /// This is NOT a property of the candidate — it records the floor the minting
    /// era stood on. The value is drawn from the ratified constant / ceremony
    /// receipt, never from request payload, and must be a member of the closed
    /// [`RATIFIED_CUSTODY_FLOORS`] set.
    ///
    /// Schema disposition (owner-ratified 2026-07-21): this field joins
    /// `m1nd-gate-receipt-v1` without a version bump — the schema stays v1 while
    /// its field set grows. Receipts minted before this floor existed are
    /// historical/void and are never re-consumed by the pipeline; an activation
    /// candidate's gates are re-minted under the floor, so the frozen canon is
    /// regenerated rather than migrated
    /// (`docs/proofs/m1nd10-g9-a1-custody-floor-ratification-20260721.md`).
    pub custody_floor: String,
    pub spec_version: String,
    pub metric_spec_digest: Option<String>,
    pub harness_fixture_digest: String,
    pub environment_digest: String,
    pub provider_id: String,
    pub provider_key_version: String,
    pub input_digests: BTreeMap<String, String>,
    pub command: String,
    pub started_at: u64,
    pub ended_at: u64,
    pub exit_code: Option<i32>,
    pub verdict: GateVerdict,
    pub findings: Vec<ReleaseFindingV1>,
    pub artifact_digests: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateReceiptV1 {
    pub schema: String,
    pub core: GateReceiptCoreV1,
    pub receipt_id: String,
    pub receipt_digest: String,
    pub signature: OpaqueSignature,
}

impl GateReceiptV1 {
    pub fn seal(
        core: GateReceiptCoreV1,
        signature: OpaqueSignature,
    ) -> Result<Self, ReleaseContractError> {
        let receipt_digest = digest_canonical(GATE_RECEIPT_DIGEST_DOMAIN, &core)?;
        let receipt = Self {
            schema: GATE_RECEIPT_SCHEMA.to_string(),
            core,
            receipt_id: format!("gate:{receipt_digest}"),
            receipt_digest,
            signature,
        };
        receipt.validate()?;
        Ok(receipt)
    }

    pub fn validate(&self) -> Result<ReleaseStructuralValidation, ReleaseContractError> {
        require_schema("gate receipt", &self.schema, GATE_RECEIPT_SCHEMA)?;
        require_digest("candidate_digest", &self.core.candidate_digest)?;
        require_ratified_custody_floor("custody_floor", &self.core.custody_floor)?;
        require_non_empty("spec_version", &self.core.spec_version)?;
        if let Some(metric_spec_digest) = &self.core.metric_spec_digest {
            require_digest("metric_spec_digest", metric_spec_digest)?;
        }
        require_digest("harness_fixture_digest", &self.core.harness_fixture_digest)?;
        require_digest("environment_digest", &self.core.environment_digest)?;
        require_non_empty("provider_id", &self.core.provider_id)?;
        require_non_empty("provider_key_version", &self.core.provider_key_version)?;
        require_non_empty("command", &self.core.command)?;
        require_non_empty_map("input_digests", &self.core.input_digests)?;
        require_non_empty_map("artifact_digests", &self.core.artifact_digests)?;
        validate_named_values("input_digests", &self.core.input_digests, true)?;
        validate_named_values("artifact_digests", &self.core.artifact_digests, true)?;
        validate_findings(&self.core.findings)?;
        if self.core.ended_at < self.core.started_at {
            return Err(ReleaseContractError::InvalidTimeWindow);
        }
        match self.core.verdict {
            GateVerdict::Pass if self.core.exit_code != Some(0) => {
                return Err(ReleaseContractError::InvalidVerdict {
                    detail: "PASS requires exit_code=0".to_string(),
                });
            }
            GateVerdict::NotRun if self.core.exit_code.is_some() => {
                return Err(ReleaseContractError::InvalidVerdict {
                    detail: "NOT_RUN cannot claim an exit code".to_string(),
                });
            }
            _ => {}
        }
        require_digest("receipt_digest", &self.receipt_digest)?;
        let expected_id = format!("gate:{}", self.receipt_digest);
        if self.receipt_id != expected_id {
            return Err(ReleaseContractError::IdentifierMismatch {
                field: "receipt_id",
            });
        }
        require_signature("signature", &self.signature)?;
        let computed = digest_canonical(GATE_RECEIPT_DIGEST_DOMAIN, &self.core)?;
        if computed != self.receipt_digest {
            return Err(ReleaseContractError::DigestMismatch {
                field: "receipt_digest",
            });
        }
        Ok(ReleaseStructuralValidation::not_authenticated())
    }

    pub fn has_open_p0_or_p1(&self) -> bool {
        has_open_p0_or_p1(&self.core.findings)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndependentAdversarialReviewCoreV1 {
    pub candidate_digest: String,
    pub threat_matrix_digest: String,
    pub provider_id: String,
    pub provider_model_version: String,
    pub provider_key_version: String,
    pub reviewed_inputs_digest: String,
    pub binding_changes: Vec<String>,
    pub started_at: u64,
    pub ended_at: u64,
    pub verdict: GateVerdict,
    pub findings: Vec<ReleaseFindingV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndependentAdversarialReviewReceiptV1 {
    pub schema: String,
    pub core: IndependentAdversarialReviewCoreV1,
    pub receipt_id: String,
    pub receipt_digest: String,
    pub signature: OpaqueSignature,
}

impl IndependentAdversarialReviewReceiptV1 {
    pub fn seal(
        core: IndependentAdversarialReviewCoreV1,
        signature: OpaqueSignature,
    ) -> Result<Self, ReleaseContractError> {
        let receipt_digest = digest_canonical(INDEPENDENT_REVIEW_RECEIPT_DIGEST_DOMAIN, &core)?;
        let receipt = Self {
            schema: INDEPENDENT_REVIEW_RECEIPT_SCHEMA.to_string(),
            core,
            receipt_id: format!("iar:{receipt_digest}"),
            receipt_digest,
            signature,
        };
        receipt.validate()?;
        Ok(receipt)
    }

    pub fn validate(&self) -> Result<ReleaseStructuralValidation, ReleaseContractError> {
        require_schema(
            "independent review receipt",
            &self.schema,
            INDEPENDENT_REVIEW_RECEIPT_SCHEMA,
        )?;
        for (field, digest) in [
            ("candidate_digest", self.core.candidate_digest.as_str()),
            (
                "threat_matrix_digest",
                self.core.threat_matrix_digest.as_str(),
            ),
            (
                "reviewed_inputs_digest",
                self.core.reviewed_inputs_digest.as_str(),
            ),
            ("receipt_digest", self.receipt_digest.as_str()),
        ] {
            require_digest(field, digest)?;
        }
        require_non_empty("provider_id", &self.core.provider_id)?;
        require_non_empty("provider_model_version", &self.core.provider_model_version)?;
        require_non_empty("provider_key_version", &self.core.provider_key_version)?;
        validate_findings(&self.core.findings)?;
        if self.core.ended_at < self.core.started_at {
            return Err(ReleaseContractError::InvalidTimeWindow);
        }
        require_signature("signature", &self.signature)?;
        let expected_id = format!("iar:{}", self.receipt_digest);
        if self.receipt_id != expected_id {
            return Err(ReleaseContractError::IdentifierMismatch {
                field: "receipt_id",
            });
        }
        let computed = digest_canonical(INDEPENDENT_REVIEW_RECEIPT_DIGEST_DOMAIN, &self.core)?;
        if computed != self.receipt_digest {
            return Err(ReleaseContractError::DigestMismatch {
                field: "receipt_digest",
            });
        }
        Ok(ReleaseStructuralValidation::not_authenticated())
    }

    pub fn has_open_p0_or_p1(&self) -> bool {
        has_open_p0_or_p1(&self.core.findings)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricSpecCoreV1 {
    pub metric_id: String,
    pub question: String,
    pub corpus_or_cohort_digest: String,
    pub ground_truth_protocol: String,
    pub unit: String,
    pub numerator: String,
    pub denominator: String,
    pub minimum_n: u64,
    pub strata: Vec<String>,
    pub environment_digest: String,
    pub workload_and_seeds_digest: String,
    pub confidence_interval: String,
    pub pass_threshold: String,
    pub non_inferiority_margin: Option<String>,
    pub command: String,
    pub artifact_retention: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricSpecV1 {
    pub schema: String,
    pub core: MetricSpecCoreV1,
    pub metric_spec_id: String,
    pub metric_spec_digest: String,
    pub ratification_signature: OpaqueSignature,
}

impl MetricSpecV1 {
    pub fn seal(
        core: MetricSpecCoreV1,
        ratification_signature: OpaqueSignature,
    ) -> Result<Self, ReleaseContractError> {
        let metric_spec_digest = digest_canonical(METRIC_SPEC_DIGEST_DOMAIN, &core)?;
        let spec = Self {
            schema: METRIC_SPEC_SCHEMA.to_string(),
            metric_spec_id: format!("metric:{metric_spec_digest}"),
            metric_spec_digest,
            core,
            ratification_signature,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn validate(&self) -> Result<ReleaseStructuralValidation, ReleaseContractError> {
        require_schema("metric spec", &self.schema, METRIC_SPEC_SCHEMA)?;
        for (field, value) in [
            ("metric_id", self.core.metric_id.as_str()),
            ("question", self.core.question.as_str()),
            (
                "ground_truth_protocol",
                self.core.ground_truth_protocol.as_str(),
            ),
            ("unit", self.core.unit.as_str()),
            ("numerator", self.core.numerator.as_str()),
            ("denominator", self.core.denominator.as_str()),
            (
                "confidence_interval",
                self.core.confidence_interval.as_str(),
            ),
            ("pass_threshold", self.core.pass_threshold.as_str()),
            ("command", self.core.command.as_str()),
            ("artifact_retention", self.core.artifact_retention.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        require_digest(
            "corpus_or_cohort_digest",
            &self.core.corpus_or_cohort_digest,
        )?;
        require_digest("environment_digest", &self.core.environment_digest)?;
        require_digest(
            "workload_and_seeds_digest",
            &self.core.workload_and_seeds_digest,
        )?;
        if self.core.minimum_n == 0 {
            return Err(ReleaseContractError::InvalidContract {
                detail: "metric minimum_n must be positive".to_string(),
            });
        }
        require_digest("metric_spec_digest", &self.metric_spec_digest)?;
        if self.metric_spec_id != format!("metric:{}", self.metric_spec_digest) {
            return Err(ReleaseContractError::IdentifierMismatch {
                field: "metric_spec_id",
            });
        }
        require_signature("ratification_signature", &self.ratification_signature)?;
        if digest_canonical(METRIC_SPEC_DIGEST_DOMAIN, &self.core)? != self.metric_spec_digest {
            return Err(ReleaseContractError::DigestMismatch {
                field: "metric_spec_digest",
            });
        }
        Ok(ReleaseStructuralValidation::not_authenticated())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseEvidenceSetV1 {
    pub candidate: ReleaseCandidateManifestV1,
    pub gate_receipts: Vec<GateReceiptV1>,
    pub independent_review: IndependentAdversarialReviewReceiptV1,
}

impl ReleaseEvidenceSetV1 {
    /// Structural convergence only. Callers must cryptographically verify every
    /// signature and key lifecycle before treating this result as a promotion
    /// authority.
    pub fn validate_convergence(
        &self,
    ) -> Result<ReleaseStructuralValidation, ReleaseContractError> {
        self.candidate.validate()?;
        self.independent_review.validate()?;
        if self.independent_review.core.candidate_digest != self.candidate.candidate_digest {
            return Err(ReleaseContractError::CandidateMismatch {
                evidence: "independent_review".to_string(),
            });
        }
        if self.independent_review.core.verdict != GateVerdict::Pass {
            return Err(ReleaseContractError::InvalidVerdict {
                detail: "independent adversarial review is not PASS".to_string(),
            });
        }
        if self.independent_review.has_open_p0_or_p1() {
            return Err(ReleaseContractError::OpenBlockingFinding {
                evidence: "independent_review".to_string(),
            });
        }

        let mut observed = BTreeSet::new();
        for receipt in &self.gate_receipts {
            receipt.validate()?;
            if receipt.core.candidate_digest != self.candidate.candidate_digest {
                return Err(ReleaseContractError::CandidateMismatch {
                    evidence: receipt.core.gate_id.to_string(),
                });
            }
            if !observed.insert(receipt.core.gate_id) {
                return Err(ReleaseContractError::DuplicateGate {
                    gate: receipt.core.gate_id,
                });
            }
            if receipt.core.verdict != GateVerdict::Pass {
                return Err(ReleaseContractError::InvalidVerdict {
                    detail: format!("{} is not PASS", receipt.core.gate_id),
                });
            }
            if receipt.has_open_p0_or_p1() {
                return Err(ReleaseContractError::OpenBlockingFinding {
                    evidence: receipt.core.gate_id.to_string(),
                });
            }
        }
        let missing = GateId::ALL
            .into_iter()
            .filter(|gate| !observed.contains(gate))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(ReleaseContractError::MissingGates { missing });
        }
        Ok(ReleaseStructuralValidation::not_authenticated())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseIntegrityDisposition {
    StructurallyValidNotCryptographicallyVerified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReleaseStructuralValidation {
    pub integrity: ReleaseIntegrityDisposition,
}

impl ReleaseStructuralValidation {
    fn not_authenticated() -> Self {
        Self {
            integrity: ReleaseIntegrityDisposition::StructurallyValidNotCryptographicallyVerified,
        }
    }
}

#[derive(Debug)]
pub enum ReleaseContractError {
    Canonical(CanonicalError),
    InvalidContract { detail: String },
    InvalidSchema { artifact: &'static str },
    DigestMismatch { field: &'static str },
    IdentifierMismatch { field: &'static str },
    InvalidTimeWindow,
    InvalidVerdict { detail: String },
    CandidateMismatch { evidence: String },
    DuplicateGate { gate: GateId },
    MissingGates { missing: Vec<GateId> },
    OpenBlockingFinding { evidence: String },
}

impl fmt::Display for ReleaseContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Canonical(error) => {
                write!(formatter, "canonical release encoding failed: {error}")
            }
            Self::InvalidContract { detail } => {
                write!(formatter, "invalid release contract: {detail}")
            }
            Self::InvalidSchema { artifact } => write!(formatter, "invalid schema for {artifact}"),
            Self::DigestMismatch { field } => write!(formatter, "digest mismatch for {field}"),
            Self::IdentifierMismatch { field } => {
                write!(
                    formatter,
                    "content-addressed identifier mismatch for {field}"
                )
            }
            Self::InvalidTimeWindow => {
                formatter.write_str("release evidence has invalid time window")
            }
            Self::InvalidVerdict { detail } => {
                write!(formatter, "invalid release verdict: {detail}")
            }
            Self::CandidateMismatch { evidence } => {
                write!(formatter, "{evidence} belongs to another release candidate")
            }
            Self::DuplicateGate { gate } => {
                write!(formatter, "duplicate current receipt for {gate}")
            }
            Self::MissingGates { missing } => {
                write!(formatter, "missing gate receipts: {missing:?}")
            }
            Self::OpenBlockingFinding { evidence } => {
                write!(formatter, "{evidence} contains an open P0/P1 finding")
            }
        }
    }
}

impl Error for ReleaseContractError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Canonical(error) => Some(error),
            _ => None,
        }
    }
}

impl From<CanonicalError> for ReleaseContractError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

fn require_schema(
    artifact: &'static str,
    observed: &str,
    expected: &str,
) -> Result<(), ReleaseContractError> {
    if observed != expected {
        return Err(ReleaseContractError::InvalidSchema { artifact });
    }
    Ok(())
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), ReleaseContractError> {
    if value.trim().is_empty() {
        return Err(ReleaseContractError::InvalidContract {
            detail: format!("required field '{field}' is empty"),
        });
    }
    Ok(())
}

/// Fail-closed: the custody floor must be present and a member of the closed
/// [`RATIFIED_CUSTODY_FLOORS`] set. Mirrors the algorithm-set precedent in
/// `m1nd-mcp::authorization_receipt_verifier` — a value outside the ratified set
/// is refused before the receipt is trusted, so a smuggled floor (e.g.
/// `"software"`) cannot claim custody the era never ratified.
fn require_ratified_custody_floor(
    field: &'static str,
    value: &str,
) -> Result<(), ReleaseContractError> {
    require_non_empty(field, value)?;
    if !is_ratified_custody_floor(value) {
        return Err(ReleaseContractError::InvalidContract {
            detail: format!(
                "field '{field}' value '{value}' is outside the ratified custody-floor set"
            ),
        });
    }
    Ok(())
}

fn require_digest(field: &'static str, value: &str) -> Result<(), ReleaseContractError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ReleaseContractError::InvalidContract {
            detail: format!("required digest '{field}' is not 64 hexadecimal characters"),
        });
    }
    Ok(())
}

fn require_signature(
    field: &'static str,
    signature: &OpaqueSignature,
) -> Result<(), ReleaseContractError> {
    if signature.is_empty() {
        return Err(ReleaseContractError::InvalidContract {
            detail: format!("required signature '{field}' is empty"),
        });
    }
    Ok(())
}

fn require_non_empty_map(
    field: &'static str,
    values: &BTreeMap<String, String>,
) -> Result<(), ReleaseContractError> {
    if values.is_empty() {
        return Err(ReleaseContractError::InvalidContract {
            detail: format!("required map '{field}' is empty"),
        });
    }
    Ok(())
}

fn validate_named_values(
    field: &'static str,
    values: &BTreeMap<String, String>,
    digest_values: bool,
) -> Result<(), ReleaseContractError> {
    for (name, value) in values {
        if name.trim().is_empty() {
            return Err(ReleaseContractError::InvalidContract {
                detail: format!("map '{field}' has an empty key"),
            });
        }
        if digest_values {
            require_digest(field, value)?;
        } else if value.trim().is_empty() {
            return Err(ReleaseContractError::InvalidContract {
                detail: format!("map '{field}' has an empty value for '{name}'"),
            });
        }
    }
    Ok(())
}

fn validate_findings(findings: &[ReleaseFindingV1]) -> Result<(), ReleaseContractError> {
    let mut ids = BTreeSet::new();
    for finding in findings {
        require_non_empty("finding_id", &finding.finding_id)?;
        require_non_empty("finding.statement", &finding.statement)?;
        require_digest("finding.evidence_digest", &finding.evidence_digest)?;
        if !ids.insert(finding.finding_id.as_str()) {
            return Err(ReleaseContractError::InvalidContract {
                detail: format!("duplicate finding id '{}'", finding.finding_id),
            });
        }
    }
    Ok(())
}

fn has_open_p0_or_p1(findings: &[ReleaseFindingV1]) -> bool {
    findings.iter().any(|finding| {
        finding.status == FindingStatus::Open
            && matches!(finding.severity, FindingSeverity::P0 | FindingSeverity::P1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: char) -> String {
        std::iter::repeat_n(byte, 64).collect()
    }

    fn signature() -> OpaqueSignature {
        OpaqueSignature::new("structural-signature-not-crypto-proof")
    }

    fn candidate() -> ReleaseCandidateManifestV1 {
        ReleaseCandidateManifestV1::seal(
            ReleaseCandidateManifestCoreV1 {
                repo_commits: BTreeMap::from([("m1nd".to_string(), "commit-1".to_string())]),
                artifact_digests: BTreeMap::from([("m1nd".to_string(), digest('a'))]),
                schema_policy_versions: BTreeMap::from([(
                    "action_catalog".to_string(),
                    "v1".to_string(),
                )]),
                tool_catalog_digest: digest('b'),
                safety_kernel_digest: digest('c'),
                previous_governance_runtime_digest: digest('d'),
                constitution_epoch_digest: digest('e'),
                autonomy_epoch_grants_digest: digest('f'),
                independence_quorum_policy_digest: digest('1'),
                intended_active_mode: ActiveMode::FullAutonomy,
                compatibility_manifest_digest: digest('2'),
                rollback_plan_digest: digest('3'),
                harness_fixture_threat_digests: BTreeMap::from([(
                    "threat_matrix".to_string(),
                    digest('4'),
                )]),
                build_environment_digest: digest('5'),
                built_at: 10,
            },
            signature(),
        )
        .unwrap()
    }

    fn gate(candidate_digest: &str, gate_id: GateId) -> GateReceiptV1 {
        GateReceiptV1::seal(
            GateReceiptCoreV1 {
                candidate_digest: candidate_digest.to_string(),
                gate_id,
                custody_floor: SECURE_ENCLAVE_CUSTODY_FLOOR_V1.to_string(),
                spec_version: "v1".to_string(),
                metric_spec_digest: Some(digest('6')),
                harness_fixture_digest: digest('7'),
                environment_digest: digest('8'),
                provider_id: "local-harness".to_string(),
                provider_key_version: "test-key-v1".to_string(),
                input_digests: BTreeMap::from([("source".to_string(), digest('9'))]),
                command: "cargo test --locked".to_string(),
                started_at: 10,
                ended_at: 20,
                exit_code: Some(0),
                verdict: GateVerdict::Pass,
                findings: vec![],
                artifact_digests: BTreeMap::from([("results".to_string(), digest('a'))]),
            },
            signature(),
        )
        .unwrap()
    }

    fn review(candidate_digest: &str) -> IndependentAdversarialReviewReceiptV1 {
        IndependentAdversarialReviewReceiptV1::seal(
            IndependentAdversarialReviewCoreV1 {
                candidate_digest: candidate_digest.to_string(),
                threat_matrix_digest: digest('4'),
                provider_id: "independent-provider".to_string(),
                provider_model_version: "model-v1".to_string(),
                provider_key_version: "key-v1".to_string(),
                reviewed_inputs_digest: digest('b'),
                binding_changes: vec![],
                started_at: 10,
                ended_at: 20,
                verdict: GateVerdict::Pass,
                findings: vec![],
            },
            signature(),
        )
        .unwrap()
    }

    #[test]
    fn candidate_digest_changes_with_any_artifact() {
        let first = candidate();
        let mut changed_core = first.core.clone();
        changed_core
            .artifact_digests
            .insert("m1nd".to_string(), digest('0'));
        let second = ReleaseCandidateManifestV1::seal(changed_core, signature()).unwrap();
        assert_ne!(first.candidate_digest, second.candidate_digest);
    }

    #[test]
    fn tampered_candidate_is_refused() {
        let mut value = candidate();
        value.core.built_at += 1;
        assert!(matches!(
            value.validate(),
            Err(ReleaseContractError::DigestMismatch {
                field: "candidate_digest"
            })
        ));
    }

    #[test]
    fn convergence_requires_exactly_all_gates_on_one_candidate() {
        let candidate = candidate();
        let evidence = ReleaseEvidenceSetV1 {
            gate_receipts: GateId::ALL
                .into_iter()
                .map(|gate_id| gate(&candidate.candidate_digest, gate_id))
                .collect(),
            independent_review: review(&candidate.candidate_digest),
            candidate,
        };
        let validation = evidence.validate_convergence().unwrap();
        assert_eq!(
            validation.integrity,
            ReleaseIntegrityDisposition::StructurallyValidNotCryptographicallyVerified
        );
    }

    #[test]
    fn receipt_from_another_candidate_is_refused() {
        let candidate = candidate();
        let mut receipts = GateId::ALL
            .into_iter()
            .map(|gate_id| gate(&candidate.candidate_digest, gate_id))
            .collect::<Vec<_>>();
        receipts[3] = gate(&digest('0'), GateId::G3);
        let error = ReleaseEvidenceSetV1 {
            independent_review: review(&candidate.candidate_digest),
            candidate,
            gate_receipts: receipts,
        }
        .validate_convergence()
        .unwrap_err();
        assert!(matches!(
            error,
            ReleaseContractError::CandidateMismatch { .. }
        ));
    }

    #[test]
    fn missing_duplicate_or_non_pass_gate_is_refused() {
        let candidate = candidate();
        let mut missing = GateId::ALL
            .into_iter()
            .take(10)
            .map(|gate_id| gate(&candidate.candidate_digest, gate_id))
            .collect::<Vec<_>>();
        let base_review = review(&candidate.candidate_digest);
        let error = ReleaseEvidenceSetV1 {
            candidate: candidate.clone(),
            gate_receipts: missing.clone(),
            independent_review: base_review.clone(),
        }
        .validate_convergence()
        .unwrap_err();
        assert!(matches!(error, ReleaseContractError::MissingGates { .. }));

        missing.push(gate(&candidate.candidate_digest, GateId::G0));
        let error = ReleaseEvidenceSetV1 {
            candidate: candidate.clone(),
            gate_receipts: missing,
            independent_review: base_review.clone(),
        }
        .validate_convergence()
        .unwrap_err();
        assert!(matches!(error, ReleaseContractError::DuplicateGate { .. }));

        let mut receipts = GateId::ALL
            .into_iter()
            .map(|gate_id| gate(&candidate.candidate_digest, gate_id))
            .collect::<Vec<_>>();
        receipts[9].core.verdict = GateVerdict::NotProven;
        receipts[9].receipt_digest =
            digest_canonical(GATE_RECEIPT_DIGEST_DOMAIN, &receipts[9].core).unwrap();
        receipts[9].receipt_id = format!("gate:{}", receipts[9].receipt_digest);
        let error = ReleaseEvidenceSetV1 {
            candidate,
            gate_receipts: receipts,
            independent_review: base_review,
        }
        .validate_convergence()
        .unwrap_err();
        assert!(matches!(error, ReleaseContractError::InvalidVerdict { .. }));
    }

    #[test]
    fn open_p0_or_p1_blocks_convergence_even_on_pass() {
        let candidate = candidate();
        let mut receipts = GateId::ALL
            .into_iter()
            .map(|gate_id| gate(&candidate.candidate_digest, gate_id))
            .collect::<Vec<_>>();
        receipts[2].core.findings.push(ReleaseFindingV1 {
            finding_id: "finding-1".to_string(),
            severity: FindingSeverity::P1,
            status: FindingStatus::Open,
            statement: "authority bypass".to_string(),
            evidence_digest: digest('d'),
        });
        receipts[2].receipt_digest =
            digest_canonical(GATE_RECEIPT_DIGEST_DOMAIN, &receipts[2].core).unwrap();
        receipts[2].receipt_id = format!("gate:{}", receipts[2].receipt_digest);
        let error = ReleaseEvidenceSetV1 {
            independent_review: review(&candidate.candidate_digest),
            candidate,
            gate_receipts: receipts,
        }
        .validate_convergence()
        .unwrap_err();
        assert!(matches!(
            error,
            ReleaseContractError::OpenBlockingFinding { .. }
        ));
    }

    #[test]
    fn structural_signature_is_required_but_not_claimed_as_verified() {
        let candidate = candidate();
        let mut receipt = gate(&candidate.candidate_digest, GateId::G0);
        receipt.signature = OpaqueSignature::new("");
        assert!(matches!(
            receipt.validate(),
            Err(ReleaseContractError::InvalidContract { .. })
        ));
    }

    #[test]
    fn gate_receipt_names_the_ratified_custody_floor() {
        let candidate = candidate();
        let receipt = gate(&candidate.candidate_digest, GateId::G0);
        assert_eq!(receipt.core.custody_floor, SECURE_ENCLAVE_CUSTODY_FLOOR_V1);
        assert!(receipt.validate().is_ok());
    }

    #[test]
    fn custody_floor_outside_the_closed_set_is_refused() {
        // Smuggling a floor the era never ratified (e.g. plain "software") must
        // fail at the structural gate, even with a self-consistent digest/id.
        let candidate = candidate();
        let mut receipt = gate(&candidate.candidate_digest, GateId::G0);
        receipt.core.custody_floor = "software".to_string();
        receipt.receipt_digest =
            digest_canonical(GATE_RECEIPT_DIGEST_DOMAIN, &receipt.core).unwrap();
        receipt.receipt_id = format!("gate:{}", receipt.receipt_digest);
        assert!(matches!(
            receipt.validate(),
            Err(ReleaseContractError::InvalidContract { .. })
        ));
    }

    #[test]
    fn empty_custody_floor_is_refused() {
        let candidate = candidate();
        let mut receipt = gate(&candidate.candidate_digest, GateId::G0);
        receipt.core.custody_floor = String::new();
        receipt.receipt_digest =
            digest_canonical(GATE_RECEIPT_DIGEST_DOMAIN, &receipt.core).unwrap();
        receipt.receipt_id = format!("gate:{}", receipt.receipt_digest);
        assert!(matches!(
            receipt.validate(),
            Err(ReleaseContractError::InvalidContract { .. })
        ));
    }
}
