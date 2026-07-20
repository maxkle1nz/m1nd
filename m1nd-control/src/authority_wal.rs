use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{
    canonical_json, digest_canonical, ActiveMode, AuthorityVariant, CanonicalError, OpaqueSignature,
};

pub const POSITIVE_AUTHORITY_TRANSACTION_SCHEMA: &str = "m1nd-positive-authority-transaction-v1";
pub const SAFETY_KERNEL_TRANSACTION_SCHEMA: &str = "m1nd-safety-kernel-transaction-v1";
pub const AUTHORITY_WAL_RECORD_SCHEMA: &str = "m1nd-authority-wal-record-v1";

pub const AUTHORITY_TRANSACTION_DIGEST_DOMAIN: &str = "m1nd-authority-transaction-v1";
pub const AUTHORITY_TRANSACTION_SIGNATURE_DOMAIN: &str = "m1nd-authority-transaction-signature-v1";
pub const AUTHORITY_WAL_PAYLOAD_DIGEST_DOMAIN: &str = "m1nd-authority-wal-payload-v1";
pub const AUTHORITY_WAL_RECORD_DIGEST_DOMAIN: &str = "m1nd-authority-wal-record-v1";
pub const AUTHORITY_WAL_RECORD_SIGNATURE_DOMAIN: &str = "m1nd-authority-wal-record-signature-v1";

const AUTHORITY_WAL_SIGNATURE_PREFIX: &[u8] = b"m1nd-authority-wal-signature-message-v1\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityKind {
    Human,
    Autonomy,
    Safety,
}

/// Bindings shared by both transaction variants. These are immutable intent,
/// subject, head, capability, and epoch facts; this layer does not authenticate
/// any of them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityTransactionBindingV1 {
    pub transaction_id: String,
    pub organism_id: String,
    pub brain_id: String,
    pub subject_id: String,
    pub action_id: String,
    pub idempotency_key: String,
    pub intent_core_ref: String,
    pub intent_digest: String,
    pub intent_canonicalization_version: String,
    pub capability_id: String,
    pub capability_kind: CapabilityKind,
    pub nonce: String,
    pub expected_head_id: Option<String>,
    pub expected_active_mode: ActiveMode,
    pub expected_activation_receipt_id: Option<String>,
    pub expected_constitution_epoch: u64,
    pub expected_autonomy_epoch: u64,
    pub expected_store_epoch: u64,
    pub sentinel_verdict_digest: Option<String>,
    pub authorization_snapshot_digest: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositiveAuthorityTransactionV1 {
    pub schema: String,
    pub binding: AuthorityTransactionBindingV1,
    pub authority_decision_digest: String,
    pub identity_role_binding_digest: String,
    pub required_authority_variant: AuthorityVariant,
    pub action_policy_registry_digest: String,
    pub classifier_decision_digest: String,
    pub expected_pending_red_set_digest: String,
    pub expected_red_latch_epoch: u64,
    pub expected_store_version: u64,
    pub expected_boundary_version: u64,
    pub expected_contract_version: u64,
    pub action_payload_digest: String,
    pub issuer: String,
    pub key_id: String,
    pub algorithm: String,
    pub transaction_digest: String,
    pub signature: OpaqueSignature,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyKernelTransactionV1 {
    pub schema: String,
    pub binding: AuthorityTransactionBindingV1,
    pub safety_intent_digest: String,
    pub safety_intent_core_ref: String,
    pub safety_intent_canonicalization_version: String,
    pub safety_attempt_id: String,
    pub sentinel_red_verdict_digest: String,
    pub red_latch_receipt_digest: String,
    pub actuator_identity_key_binary_policy_digest: String,
    pub allowed_negative_actions_digest: String,
    pub affected_grants_scope_digest: String,
    pub rollback_candidate_plan_digest: String,
    pub expected_next_autonomy_epoch: u64,
    pub positive_authority_decision_forbidden: bool,
    pub issuer: String,
    pub key_id: String,
    pub algorithm: String,
    pub transaction_digest: String,
    pub signature: OpaqueSignature,
}

/// The sovereign transaction is exactly one discriminated authority variant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "transaction_variant",
    content = "transaction",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum AuthorityTransactionV1 {
    PositiveAuthority(PositiveAuthorityTransactionV1),
    SafetyKernel(SafetyKernelTransactionV1),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorityWalPhase {
    Prepare,
    Provisional,
    Commit,
    Abort,
}

impl AuthorityWalPhase {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Commit | Self::Abort)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityWalPrepareV1 {
    pub transaction: AuthorityTransactionV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityWalProvisionalV1 {
    pub provisional_effects_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityWalCommitV1 {
    pub committed_at: u64,
    pub protected_time_evidence_digest: String,
    pub authorization_snapshot_digest: String,
    pub terminal_outcome_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityWalAbortV1 {
    pub aborted_at: u64,
    pub reason_digest: String,
    pub terminal_outcome_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "payload_kind",
    content = "payload",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum AuthorityWalPayloadV1 {
    Prepare(Box<AuthorityWalPrepareV1>),
    Provisional(AuthorityWalProvisionalV1),
    Commit(AuthorityWalCommitV1),
    Abort(AuthorityWalAbortV1),
}

impl AuthorityWalPayloadV1 {
    pub const fn phase(&self) -> AuthorityWalPhase {
        match self {
            Self::Prepare(_) => AuthorityWalPhase::Prepare,
            Self::Provisional(_) => AuthorityWalPhase::Provisional,
            Self::Commit(_) => AuthorityWalPhase::Commit,
            Self::Abort(_) => AuthorityWalPhase::Abort,
        }
    }
}

/// One newline-framed AuthorityWAL record. `record_digest` is the canonical
/// self-hash excluding only itself and the record's opaque signature. The
/// previous digest forms the append-only hash chain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityWalRecordV1 {
    pub schema: String,
    pub sequence: u64,
    pub phase: AuthorityWalPhase,
    pub transaction_id: String,
    pub idempotency_key: String,
    pub intent_core_ref: String,
    pub intent_digest: String,
    pub expected_head_id: Option<String>,
    pub expected_constitution_epoch: u64,
    pub expected_autonomy_epoch: u64,
    pub expected_store_epoch: u64,
    pub transaction_digest: String,
    pub authorization_snapshot_digest: String,
    pub payload: AuthorityWalPayloadV1,
    pub payload_digest: String,
    pub previous_record_digest: Option<String>,
    pub recorded_at: u64,
    pub issuer: String,
    pub key_id: String,
    pub algorithm: String,
    pub record_digest: String,
    pub signature: OpaqueSignature,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityWalIntegrityDisposition {
    OpaqueSignaturePresentUnverified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityTransactionStructuralValidation {
    pub transaction_digest: String,
    pub integrity: AuthorityWalIntegrityDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityWalRecordStructuralValidation {
    pub payload_digest: String,
    pub record_digest: String,
    pub integrity: AuthorityWalIntegrityDisposition,
}

#[derive(Debug, Error)]
pub enum AuthorityWalContractError {
    #[error("unsupported {contract} schema '{actual}'")]
    Schema {
        contract: &'static str,
        actual: String,
    },
    #[error("required field '{field}' is empty")]
    EmptyRequired { field: &'static str },
    #[error("digest field '{field}' is not a lowercase SHA-256 hex digest")]
    InvalidDigest { field: &'static str },
    #[error("transaction lifetime is invalid: issued_at={issued_at}, expires_at={expires_at}")]
    InvalidLifetime { issued_at: u64, expires_at: u64 },
    #[error("authority variant {variant:?} is not positive sovereign authority")]
    NonPositiveAuthorityVariant { variant: AuthorityVariant },
    #[error("capability kind {kind:?} is incompatible with authority variant {variant:?}")]
    CapabilityVariantMismatch {
        kind: CapabilityKind,
        variant: AuthorityVariant,
    },
    #[error("SAFETY_KERNEL transaction requires a SAFETY capability")]
    SafetyCapabilityRequired,
    #[error("SAFETY_KERNEL transaction must forbid a positive AuthorityDecision")]
    PositiveAuthorityDecisionNotForbidden,
    #[error("SAFETY_KERNEL sentinel binding does not equal its RED verdict digest")]
    SafetySentinelMismatch,
    #[error("SAFETY_KERNEL next autonomy epoch must be exactly current epoch + 1")]
    InvalidNextAutonomyEpoch,
    #[error("transaction digest mismatch: expected {expected}, observed {observed}")]
    TransactionDigestMismatch { expected: String, observed: String },
    #[error("WAL record sequence must be greater than zero")]
    InvalidSequence,
    #[error("WAL phase {phase:?} does not match payload phase {payload_phase:?}")]
    PhasePayloadMismatch {
        phase: AuthorityWalPhase,
        payload_phase: AuthorityWalPhase,
    },
    #[error("payload digest mismatch: expected {expected}, observed {observed}")]
    PayloadDigestMismatch { expected: String, observed: String },
    #[error("record digest mismatch: expected {expected}, observed {observed}")]
    RecordDigestMismatch { expected: String, observed: String },
    #[error("WAL {field} binding does not match the prepared transaction")]
    TransactionBindingMismatch { field: &'static str },
    #[error("COMMIT precedes transaction issue time or reaches/exceeds expiry")]
    InvalidCommitTime,
    #[error("terminal timestamp {terminal_at} exceeds record timestamp {recorded_at}")]
    TerminalAfterRecord { terminal_at: u64, recorded_at: u64 },
    #[error(transparent)]
    Canonical(#[from] CanonicalError),
}

impl AuthorityTransactionV1 {
    pub fn binding(&self) -> &AuthorityTransactionBindingV1 {
        match self {
            Self::PositiveAuthority(transaction) => &transaction.binding,
            Self::SafetyKernel(transaction) => &transaction.binding,
        }
    }

    pub fn transaction_digest(&self) -> &str {
        match self {
            Self::PositiveAuthority(transaction) => &transaction.transaction_digest,
            Self::SafetyKernel(transaction) => &transaction.transaction_digest,
        }
    }

    pub fn signature(&self) -> &OpaqueSignature {
        match self {
            Self::PositiveAuthority(transaction) => &transaction.signature,
            Self::SafetyKernel(transaction) => &transaction.signature,
        }
    }

    pub fn compute_transaction_digest(&self) -> Result<String, CanonicalError> {
        let mut value = serde_json::to_value(self)?;
        if let Value::Object(root) = &mut value {
            if let Some(Value::Object(transaction)) = root.get_mut("transaction") {
                transaction.remove("transaction_digest");
                transaction.remove("signature");
            }
        }
        digest_canonical(AUTHORITY_TRANSACTION_DIGEST_DOMAIN, &value)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        let digest = self.compute_transaction_digest()?;
        match self {
            Self::PositiveAuthority(transaction) => transaction.transaction_digest = digest,
            Self::SafetyKernel(transaction) => transaction.transaction_digest = digest,
        }
        Ok(())
    }

    /// Canonical non-circular signature body: the complete discriminated outer
    /// transaction including its already-sealed `transaction_digest`, with
    /// only the opaque `signature` removed. The digest itself was computed from
    /// the same body with both digest and signature absent.
    pub fn canonical_signature_payload(&self) -> Result<Vec<u8>, CanonicalError> {
        let mut value = serde_json::to_value(self)?;
        if let Value::Object(root) = &mut value {
            if let Some(Value::Object(transaction)) = root.get_mut("transaction") {
                transaction.remove("signature");
            }
        }
        canonical_json(&value)
    }

    pub fn validate(
        &self,
    ) -> Result<AuthorityTransactionStructuralValidation, AuthorityWalContractError> {
        validate_binding(self.binding())?;
        match self {
            Self::PositiveAuthority(transaction) => validate_positive(transaction)?,
            Self::SafetyKernel(transaction) => validate_safety(transaction)?,
        }
        validate_signature_fields(self)?;
        require_digest("transaction_digest", self.transaction_digest())?;
        let expected = self.compute_transaction_digest()?;
        if expected != self.transaction_digest() {
            return Err(AuthorityWalContractError::TransactionDigestMismatch {
                expected,
                observed: self.transaction_digest().to_string(),
            });
        }
        Ok(AuthorityTransactionStructuralValidation {
            transaction_digest: expected,
            integrity: AuthorityWalIntegrityDisposition::OpaqueSignaturePresentUnverified,
        })
    }
}

impl AuthorityWalRecordV1 {
    /// Build an unchained record draft from one already sealed transaction.
    /// The durable writer owns `sequence`, `previous_record_digest`, and sealing.
    pub fn draft(
        transaction: &AuthorityTransactionV1,
        payload: AuthorityWalPayloadV1,
        recorded_at: u64,
        issuer: impl Into<String>,
        key_id: impl Into<String>,
        algorithm: impl Into<String>,
        signature: OpaqueSignature,
    ) -> Self {
        let binding = transaction.binding();
        Self {
            schema: AUTHORITY_WAL_RECORD_SCHEMA.to_string(),
            sequence: 0,
            phase: payload.phase(),
            transaction_id: binding.transaction_id.clone(),
            idempotency_key: binding.idempotency_key.clone(),
            intent_core_ref: binding.intent_core_ref.clone(),
            intent_digest: binding.intent_digest.clone(),
            expected_head_id: binding.expected_head_id.clone(),
            expected_constitution_epoch: binding.expected_constitution_epoch,
            expected_autonomy_epoch: binding.expected_autonomy_epoch,
            expected_store_epoch: binding.expected_store_epoch,
            transaction_digest: transaction.transaction_digest().to_string(),
            authorization_snapshot_digest: binding.authorization_snapshot_digest.clone(),
            payload,
            payload_digest: String::new(),
            previous_record_digest: None,
            recorded_at,
            issuer: issuer.into(),
            key_id: key_id.into(),
            algorithm: algorithm.into(),
            record_digest: String::new(),
            signature,
        }
    }

    pub fn compute_payload_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(AUTHORITY_WAL_PAYLOAD_DIGEST_DOMAIN, &self.payload)
    }

    pub fn compute_record_digest(&self) -> Result<String, CanonicalError> {
        let mut value = serde_json::to_value(self)?;
        if let Value::Object(record) = &mut value {
            record.remove("record_digest");
            record.remove("signature");
        }
        digest_canonical(AUTHORITY_WAL_RECORD_DIGEST_DOMAIN, &value)
    }

    /// Exact bytes a WAL signer/verifier must authenticate.
    ///
    /// This is the complete canonical record body with only the signature
    /// field removed, wrapped in a length-delimited domain. In particular it
    /// includes the writer-assigned sequence and previous-record digest, so a
    /// phase artifact created before append cannot be replayed as a valid WAL
    /// signature.
    pub fn canonical_signature_message(&self) -> Result<Vec<u8>, CanonicalError> {
        let mut value = serde_json::to_value(self)?;
        if let Value::Object(record) = &mut value {
            record.remove("signature");
        }
        let canonical = crate::canonical_json(&value)?;
        let mut message = Vec::with_capacity(
            AUTHORITY_WAL_SIGNATURE_PREFIX.len()
                + 8
                + AUTHORITY_WAL_RECORD_SIGNATURE_DOMAIN.len()
                + 8
                + canonical.len(),
        );
        message.extend_from_slice(AUTHORITY_WAL_SIGNATURE_PREFIX);
        message
            .extend_from_slice(&(AUTHORITY_WAL_RECORD_SIGNATURE_DOMAIN.len() as u64).to_be_bytes());
        message.extend_from_slice(AUTHORITY_WAL_RECORD_SIGNATURE_DOMAIN.as_bytes());
        message.extend_from_slice(&(canonical.len() as u64).to_be_bytes());
        message.extend_from_slice(&canonical);
        Ok(message)
    }

    pub fn assign_chain_and_seal(
        &mut self,
        sequence: u64,
        previous_record_digest: Option<String>,
    ) -> Result<(), CanonicalError> {
        self.sequence = sequence;
        self.previous_record_digest = previous_record_digest;
        self.payload_digest = self.compute_payload_digest()?;
        self.record_digest = self.compute_record_digest()?;
        Ok(())
    }

    pub fn validate(
        &self,
    ) -> Result<AuthorityWalRecordStructuralValidation, AuthorityWalContractError> {
        require_schema(
            "AuthorityWAL record",
            &self.schema,
            AUTHORITY_WAL_RECORD_SCHEMA,
        )?;
        if self.sequence == 0 {
            return Err(AuthorityWalContractError::InvalidSequence);
        }
        for (field, value) in [
            ("transaction_id", self.transaction_id.as_str()),
            ("idempotency_key", self.idempotency_key.as_str()),
            ("intent_core_ref", self.intent_core_ref.as_str()),
            ("issuer", self.issuer.as_str()),
            ("key_id", self.key_id.as_str()),
            ("algorithm", self.algorithm.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        require_optional_non_empty("expected_head_id", self.expected_head_id.as_deref())?;
        for (field, digest) in [
            ("intent_digest", self.intent_digest.as_str()),
            ("transaction_digest", self.transaction_digest.as_str()),
            (
                "authorization_snapshot_digest",
                self.authorization_snapshot_digest.as_str(),
            ),
            ("payload_digest", self.payload_digest.as_str()),
            ("record_digest", self.record_digest.as_str()),
        ] {
            require_digest(field, digest)?;
        }
        require_optional_digest(
            "previous_record_digest",
            self.previous_record_digest.as_deref(),
        )?;
        if self.signature.is_empty() {
            return Err(AuthorityWalContractError::EmptyRequired { field: "signature" });
        }
        let payload_phase = self.payload.phase();
        if payload_phase != self.phase {
            return Err(AuthorityWalContractError::PhasePayloadMismatch {
                phase: self.phase,
                payload_phase,
            });
        }
        validate_payload_shape(&self.payload, self.recorded_at)?;

        let payload_digest = self.compute_payload_digest()?;
        if payload_digest != self.payload_digest {
            return Err(AuthorityWalContractError::PayloadDigestMismatch {
                expected: payload_digest,
                observed: self.payload_digest.clone(),
            });
        }
        let record_digest = self.compute_record_digest()?;
        if record_digest != self.record_digest {
            return Err(AuthorityWalContractError::RecordDigestMismatch {
                expected: record_digest,
                observed: self.record_digest.clone(),
            });
        }
        Ok(AuthorityWalRecordStructuralValidation {
            payload_digest,
            record_digest,
            integrity: AuthorityWalIntegrityDisposition::OpaqueSignaturePresentUnverified,
        })
    }

    pub fn validate_against_transaction(
        &self,
        transaction: &AuthorityTransactionV1,
    ) -> Result<AuthorityWalRecordStructuralValidation, AuthorityWalContractError> {
        let validation = self.validate()?;
        transaction.validate()?;
        let binding = transaction.binding();
        ensure_binding(
            "transaction_id",
            self.transaction_id == binding.transaction_id,
        )?;
        ensure_binding(
            "idempotency_key",
            self.idempotency_key == binding.idempotency_key,
        )?;
        ensure_binding(
            "intent_core_ref",
            self.intent_core_ref == binding.intent_core_ref,
        )?;
        ensure_binding("intent_digest", self.intent_digest == binding.intent_digest)?;
        ensure_binding(
            "expected_head_id",
            self.expected_head_id == binding.expected_head_id,
        )?;
        ensure_binding(
            "expected_constitution_epoch",
            self.expected_constitution_epoch == binding.expected_constitution_epoch,
        )?;
        ensure_binding(
            "expected_autonomy_epoch",
            self.expected_autonomy_epoch == binding.expected_autonomy_epoch,
        )?;
        ensure_binding(
            "expected_store_epoch",
            self.expected_store_epoch == binding.expected_store_epoch,
        )?;
        ensure_binding(
            "transaction_digest",
            self.transaction_digest == transaction.transaction_digest(),
        )?;
        ensure_binding(
            "authorization_snapshot_digest",
            self.authorization_snapshot_digest == binding.authorization_snapshot_digest,
        )?;
        if let AuthorityWalPayloadV1::Prepare(payload) = &self.payload {
            ensure_binding(
                "prepare.transaction",
                &payload.as_ref().transaction == transaction,
            )?;
        }
        if let AuthorityWalPayloadV1::Commit(payload) = &self.payload {
            ensure_binding(
                "commit.authorization_snapshot_digest",
                payload.authorization_snapshot_digest == binding.authorization_snapshot_digest,
            )?;
            if payload.committed_at < binding.issued_at
                || payload.committed_at >= binding.expires_at
            {
                return Err(AuthorityWalContractError::InvalidCommitTime);
            }
        }
        Ok(validation)
    }
}

fn validate_binding(
    binding: &AuthorityTransactionBindingV1,
) -> Result<(), AuthorityWalContractError> {
    for (field, value) in [
        ("transaction_id", binding.transaction_id.as_str()),
        ("organism_id", binding.organism_id.as_str()),
        ("brain_id", binding.brain_id.as_str()),
        ("subject_id", binding.subject_id.as_str()),
        ("action_id", binding.action_id.as_str()),
        ("idempotency_key", binding.idempotency_key.as_str()),
        ("intent_core_ref", binding.intent_core_ref.as_str()),
        (
            "intent_canonicalization_version",
            binding.intent_canonicalization_version.as_str(),
        ),
        ("capability_id", binding.capability_id.as_str()),
        ("nonce", binding.nonce.as_str()),
    ] {
        require_non_empty(field, value)?;
    }
    require_optional_non_empty("expected_head_id", binding.expected_head_id.as_deref())?;
    require_optional_non_empty(
        "expected_activation_receipt_id",
        binding.expected_activation_receipt_id.as_deref(),
    )?;
    require_digest("intent_digest", &binding.intent_digest)?;
    require_optional_digest(
        "sentinel_verdict_digest",
        binding.sentinel_verdict_digest.as_deref(),
    )?;
    require_digest(
        "authorization_snapshot_digest",
        &binding.authorization_snapshot_digest,
    )?;
    if binding.issued_at >= binding.expires_at {
        return Err(AuthorityWalContractError::InvalidLifetime {
            issued_at: binding.issued_at,
            expires_at: binding.expires_at,
        });
    }
    Ok(())
}

fn validate_positive(
    transaction: &PositiveAuthorityTransactionV1,
) -> Result<(), AuthorityWalContractError> {
    require_schema(
        "positive authority transaction",
        &transaction.schema,
        POSITIVE_AUTHORITY_TRANSACTION_SCHEMA,
    )?;
    let variant = transaction.required_authority_variant;
    if !variant.is_positive_sovereign() {
        return Err(AuthorityWalContractError::NonPositiveAuthorityVariant { variant });
    }
    let compatible = matches!(
        (transaction.binding.capability_kind, variant),
        (CapabilityKind::Human, AuthorityVariant::Human)
            | (
                CapabilityKind::Autonomy,
                AuthorityVariant::Policy | AuthorityVariant::AgentQuorum
            )
    );
    if !compatible {
        return Err(AuthorityWalContractError::CapabilityVariantMismatch {
            kind: transaction.binding.capability_kind,
            variant,
        });
    }
    for (field, digest) in [
        (
            "authority_decision_digest",
            transaction.authority_decision_digest.as_str(),
        ),
        (
            "identity_role_binding_digest",
            transaction.identity_role_binding_digest.as_str(),
        ),
        (
            "action_policy_registry_digest",
            transaction.action_policy_registry_digest.as_str(),
        ),
        (
            "classifier_decision_digest",
            transaction.classifier_decision_digest.as_str(),
        ),
        (
            "expected_pending_red_set_digest",
            transaction.expected_pending_red_set_digest.as_str(),
        ),
        (
            "action_payload_digest",
            transaction.action_payload_digest.as_str(),
        ),
    ] {
        require_digest(field, digest)?;
    }
    Ok(())
}

fn validate_safety(
    transaction: &SafetyKernelTransactionV1,
) -> Result<(), AuthorityWalContractError> {
    require_schema(
        "safety kernel transaction",
        &transaction.schema,
        SAFETY_KERNEL_TRANSACTION_SCHEMA,
    )?;
    if transaction.binding.capability_kind != CapabilityKind::Safety {
        return Err(AuthorityWalContractError::SafetyCapabilityRequired);
    }
    if !transaction.positive_authority_decision_forbidden {
        return Err(AuthorityWalContractError::PositiveAuthorityDecisionNotForbidden);
    }
    if transaction.binding.sentinel_verdict_digest.as_deref()
        != Some(transaction.sentinel_red_verdict_digest.as_str())
    {
        return Err(AuthorityWalContractError::SafetySentinelMismatch);
    }
    if transaction.binding.expected_autonomy_epoch.checked_add(1)
        != Some(transaction.expected_next_autonomy_epoch)
    {
        return Err(AuthorityWalContractError::InvalidNextAutonomyEpoch);
    }
    for (field, value) in [
        (
            "safety_intent_core_ref",
            transaction.safety_intent_core_ref.as_str(),
        ),
        (
            "safety_intent_canonicalization_version",
            transaction.safety_intent_canonicalization_version.as_str(),
        ),
        ("safety_attempt_id", transaction.safety_attempt_id.as_str()),
    ] {
        require_non_empty(field, value)?;
    }
    for (field, digest) in [
        (
            "safety_intent_digest",
            transaction.safety_intent_digest.as_str(),
        ),
        (
            "sentinel_red_verdict_digest",
            transaction.sentinel_red_verdict_digest.as_str(),
        ),
        (
            "red_latch_receipt_digest",
            transaction.red_latch_receipt_digest.as_str(),
        ),
        (
            "actuator_identity_key_binary_policy_digest",
            transaction
                .actuator_identity_key_binary_policy_digest
                .as_str(),
        ),
        (
            "allowed_negative_actions_digest",
            transaction.allowed_negative_actions_digest.as_str(),
        ),
        (
            "affected_grants_scope_digest",
            transaction.affected_grants_scope_digest.as_str(),
        ),
        (
            "rollback_candidate_plan_digest",
            transaction.rollback_candidate_plan_digest.as_str(),
        ),
    ] {
        require_digest(field, digest)?;
    }
    Ok(())
}

fn validate_signature_fields(
    transaction: &AuthorityTransactionV1,
) -> Result<(), AuthorityWalContractError> {
    let (issuer, key_id, algorithm) = match transaction {
        AuthorityTransactionV1::PositiveAuthority(transaction) => (
            transaction.issuer.as_str(),
            transaction.key_id.as_str(),
            transaction.algorithm.as_str(),
        ),
        AuthorityTransactionV1::SafetyKernel(transaction) => (
            transaction.issuer.as_str(),
            transaction.key_id.as_str(),
            transaction.algorithm.as_str(),
        ),
    };
    for (field, value) in [
        ("issuer", issuer),
        ("key_id", key_id),
        ("algorithm", algorithm),
    ] {
        require_non_empty(field, value)?;
    }
    if transaction.signature().is_empty() {
        return Err(AuthorityWalContractError::EmptyRequired { field: "signature" });
    }
    Ok(())
}

fn validate_payload_shape(
    payload: &AuthorityWalPayloadV1,
    recorded_at: u64,
) -> Result<(), AuthorityWalContractError> {
    match payload {
        AuthorityWalPayloadV1::Prepare(payload) => {
            payload.transaction.validate()?;
        }
        AuthorityWalPayloadV1::Provisional(payload) => {
            require_digest(
                "provisional_effects_digest",
                &payload.provisional_effects_digest,
            )?;
        }
        AuthorityWalPayloadV1::Commit(payload) => {
            for (field, digest) in [
                (
                    "protected_time_evidence_digest",
                    payload.protected_time_evidence_digest.as_str(),
                ),
                (
                    "commit.authorization_snapshot_digest",
                    payload.authorization_snapshot_digest.as_str(),
                ),
                (
                    "terminal_outcome_digest",
                    payload.terminal_outcome_digest.as_str(),
                ),
            ] {
                require_digest(field, digest)?;
            }
            ensure_terminal_not_after_record(payload.committed_at, recorded_at)?;
        }
        AuthorityWalPayloadV1::Abort(payload) => {
            require_digest("reason_digest", &payload.reason_digest)?;
            require_digest("terminal_outcome_digest", &payload.terminal_outcome_digest)?;
            ensure_terminal_not_after_record(payload.aborted_at, recorded_at)?;
        }
    }
    Ok(())
}

fn ensure_terminal_not_after_record(
    terminal_at: u64,
    recorded_at: u64,
) -> Result<(), AuthorityWalContractError> {
    if terminal_at > recorded_at {
        return Err(AuthorityWalContractError::TerminalAfterRecord {
            terminal_at,
            recorded_at,
        });
    }
    Ok(())
}

fn ensure_binding(field: &'static str, equal: bool) -> Result<(), AuthorityWalContractError> {
    if !equal {
        return Err(AuthorityWalContractError::TransactionBindingMismatch { field });
    }
    Ok(())
}

fn require_schema(
    contract: &'static str,
    actual: &str,
    expected: &str,
) -> Result<(), AuthorityWalContractError> {
    if actual != expected {
        return Err(AuthorityWalContractError::Schema {
            contract,
            actual: actual.to_string(),
        });
    }
    Ok(())
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), AuthorityWalContractError> {
    if value.is_empty() {
        return Err(AuthorityWalContractError::EmptyRequired { field });
    }
    Ok(())
}

fn require_optional_non_empty(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), AuthorityWalContractError> {
    if value.is_some_and(str::is_empty) {
        return Err(AuthorityWalContractError::EmptyRequired { field });
    }
    Ok(())
}

fn require_optional_digest(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), AuthorityWalContractError> {
    if let Some(value) = value {
        require_digest(field, value)?;
    }
    Ok(())
}

fn require_digest(field: &'static str, value: &str) -> Result<(), AuthorityWalContractError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AuthorityWalContractError::InvalidDigest { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};

    use super::*;

    const ISSUED_AT: u64 = 1_000;
    const EXPIRES_AT: u64 = 3_000;

    fn hash(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn signature(value: &str) -> OpaqueSignature {
        OpaqueSignature::new(value)
    }

    fn binding(kind: CapabilityKind) -> AuthorityTransactionBindingV1 {
        AuthorityTransactionBindingV1 {
            transaction_id: "tx-1".to_string(),
            organism_id: "organism-1".to_string(),
            brain_id: "brain-1".to_string(),
            subject_id: "subject-1".to_string(),
            action_id: "land".to_string(),
            idempotency_key: "idempotency-1".to_string(), // gitleaks:allow
            intent_core_ref: "intent:sha256:fixture".to_string(),
            intent_digest: hash('a'),
            intent_canonicalization_version: "m1nd-canonical-json-v1".to_string(),
            capability_id: "capability-1".to_string(),
            capability_kind: kind,
            nonce: "nonce-1".to_string(),
            expected_head_id: Some("head-1".to_string()),
            expected_active_mode: ActiveMode::HumanGated,
            expected_activation_receipt_id: None,
            expected_constitution_epoch: 2,
            expected_autonomy_epoch: 7,
            expected_store_epoch: 11,
            sentinel_verdict_digest: None,
            authorization_snapshot_digest: hash('b'),
            issued_at: ISSUED_AT,
            expires_at: EXPIRES_AT,
        }
    }

    fn positive_fixture() -> AuthorityTransactionV1 {
        let mut transaction =
            AuthorityTransactionV1::PositiveAuthority(PositiveAuthorityTransactionV1 {
                schema: POSITIVE_AUTHORITY_TRANSACTION_SCHEMA.to_string(),
                binding: binding(CapabilityKind::Human),
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
                signature: signature("opaque-positive"),
            });
        transaction.seal().unwrap();
        transaction
    }

    fn safety_fixture() -> AuthorityTransactionV1 {
        let mut safety_binding = binding(CapabilityKind::Safety);
        safety_binding.action_id = "freeze-and-rollback".to_string();
        safety_binding.sentinel_verdict_digest = Some(hash('3'));
        let mut transaction = AuthorityTransactionV1::SafetyKernel(SafetyKernelTransactionV1 {
            schema: SAFETY_KERNEL_TRANSACTION_SCHEMA.to_string(),
            binding: safety_binding,
            safety_intent_digest: hash('4'),
            safety_intent_core_ref: "intent:safety:fixture".to_string(),
            safety_intent_canonicalization_version: "m1nd-canonical-json-v1".to_string(),
            safety_attempt_id: "safety-attempt-1".to_string(),
            sentinel_red_verdict_digest: hash('3'),
            red_latch_receipt_digest: hash('5'),
            actuator_identity_key_binary_policy_digest: hash('6'),
            allowed_negative_actions_digest: hash('7'),
            affected_grants_scope_digest: hash('8'),
            rollback_candidate_plan_digest: hash('9'),
            expected_next_autonomy_epoch: 8,
            positive_authority_decision_forbidden: true,
            issuer: "safety-kernel-1".to_string(),
            key_id: "safety-key-1".to_string(),
            algorithm: "opaque-test-algorithm".to_string(),
            transaction_digest: hash('0'),
            signature: signature("opaque-safety"),
        });
        transaction.seal().unwrap();
        transaction
    }

    fn payload_for(
        phase: AuthorityWalPhase,
        transaction: &AuthorityTransactionV1,
    ) -> AuthorityWalPayloadV1 {
        match phase {
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
        }
    }

    fn record_fixture(
        phase: AuthorityWalPhase,
        transaction: &AuthorityTransactionV1,
        sequence: u64,
        previous: Option<String>,
    ) -> AuthorityWalRecordV1 {
        let mut record = AuthorityWalRecordV1::draft(
            transaction,
            payload_for(phase, transaction),
            1_600,
            "owner-1",
            "owner-key-1",
            "opaque-test-algorithm",
            signature("opaque-record"),
        );
        record.assign_chain_and_seal(sequence, previous).unwrap();
        record
    }

    fn assert_unknown_field_rejected<T: Serialize + DeserializeOwned>(value: &T) {
        let mut wire = serde_json::to_value(value).unwrap();
        wire.as_object_mut()
            .unwrap()
            .insert("unexpected".to_string(), json!(true));
        assert!(serde_json::from_value::<T>(wire).is_err());
    }

    #[test]
    fn both_discriminated_transaction_variants_validate_without_authenticating() {
        for transaction in [positive_fixture(), safety_fixture()] {
            let validation = transaction.validate().unwrap();
            assert_eq!(
                validation.transaction_digest,
                transaction.transaction_digest()
            );
            assert_eq!(
                validation.integrity,
                AuthorityWalIntegrityDisposition::OpaqueSignaturePresentUnverified
            );
        }
        let positive = serde_json::to_value(positive_fixture()).unwrap();
        let safety = serde_json::to_value(safety_fixture()).unwrap();
        assert_eq!(positive["transaction_variant"], "POSITIVE_AUTHORITY");
        assert_eq!(safety["transaction_variant"], "SAFETY_KERNEL");
        assert!(positive.get("transaction").is_some());
        assert!(safety.get("transaction").is_some());
    }

    #[test]
    fn positive_variant_and_capability_are_fail_closed() {
        let mut ordinary = positive_fixture();
        let AuthorityTransactionV1::PositiveAuthority(transaction) = &mut ordinary else {
            unreachable!()
        };
        transaction.required_authority_variant = AuthorityVariant::Ordinary;
        ordinary.seal().unwrap();
        assert!(matches!(
            ordinary.validate(),
            Err(AuthorityWalContractError::NonPositiveAuthorityVariant { .. })
        ));

        let mut wrong_capability = positive_fixture();
        let AuthorityTransactionV1::PositiveAuthority(transaction) = &mut wrong_capability else {
            unreachable!()
        };
        transaction.binding.capability_kind = CapabilityKind::Safety;
        wrong_capability.seal().unwrap();
        assert!(matches!(
            wrong_capability.validate(),
            Err(AuthorityWalContractError::CapabilityVariantMismatch { .. })
        ));
    }

    #[test]
    fn safety_variant_requires_red_safety_only_and_exact_next_epoch() {
        let mut allows_positive = safety_fixture();
        let AuthorityTransactionV1::SafetyKernel(transaction) = &mut allows_positive else {
            unreachable!()
        };
        transaction.positive_authority_decision_forbidden = false;
        allows_positive.seal().unwrap();
        assert!(matches!(
            allows_positive.validate(),
            Err(AuthorityWalContractError::PositiveAuthorityDecisionNotForbidden)
        ));

        let mut wrong_red = safety_fixture();
        let AuthorityTransactionV1::SafetyKernel(transaction) = &mut wrong_red else {
            unreachable!()
        };
        transaction.binding.sentinel_verdict_digest = Some(hash('f'));
        wrong_red.seal().unwrap();
        assert!(matches!(
            wrong_red.validate(),
            Err(AuthorityWalContractError::SafetySentinelMismatch)
        ));

        let mut stale_epoch = safety_fixture();
        let AuthorityTransactionV1::SafetyKernel(transaction) = &mut stale_epoch else {
            unreachable!()
        };
        transaction.expected_next_autonomy_epoch = 9;
        stale_epoch.seal().unwrap();
        assert!(matches!(
            stale_epoch.validate(),
            Err(AuthorityWalContractError::InvalidNextAutonomyEpoch)
        ));
    }

    #[test]
    fn all_four_wal_record_payloads_validate_and_bind_the_transaction() {
        let transaction = positive_fixture();
        let mut previous = None;
        for (index, phase) in [
            AuthorityWalPhase::Prepare,
            AuthorityWalPhase::Provisional,
            AuthorityWalPhase::Commit,
            AuthorityWalPhase::Abort,
        ]
        .into_iter()
        .enumerate()
        {
            let record = record_fixture(phase, &transaction, (index + 1) as u64, previous);
            let validation = record.validate_against_transaction(&transaction).unwrap();
            assert_eq!(
                validation.integrity,
                AuthorityWalIntegrityDisposition::OpaqueSignaturePresentUnverified
            );
            assert_eq!(record.phase, record.payload.phase());
            previous = Some(record.record_digest);
        }
    }

    #[test]
    fn swapped_payload_and_transaction_bytes_are_detected() {
        let transaction = positive_fixture();
        let mut record = record_fixture(AuthorityWalPhase::Provisional, &transaction, 1, None);
        let AuthorityWalPayloadV1::Provisional(payload) = &mut record.payload else {
            unreachable!()
        };
        payload.provisional_effects_digest = hash('f');
        assert!(matches!(
            record.validate(),
            Err(AuthorityWalContractError::PayloadDigestMismatch { .. })
        ));

        let mut changed_transaction = transaction;
        let AuthorityTransactionV1::PositiveAuthority(positive) = &mut changed_transaction else {
            unreachable!()
        };
        positive.action_payload_digest = hash('f');
        assert!(matches!(
            changed_transaction.validate(),
            Err(AuthorityWalContractError::TransactionDigestMismatch { .. })
        ));
    }

    #[test]
    fn resealed_record_still_cannot_swap_a_transaction_binding() {
        let transaction = positive_fixture();
        let mut record = record_fixture(AuthorityWalPhase::Provisional, &transaction, 1, None);
        record.intent_digest = hash('f');
        record.assign_chain_and_seal(1, None).unwrap();
        assert!(matches!(
            record.validate_against_transaction(&transaction),
            Err(AuthorityWalContractError::TransactionBindingMismatch {
                field: "intent_digest"
            })
        ));
    }

    #[test]
    fn phase_payload_mismatch_and_invalid_commit_window_are_refused() {
        let transaction = positive_fixture();
        let mut wrong_phase = record_fixture(AuthorityWalPhase::Provisional, &transaction, 1, None);
        wrong_phase.phase = AuthorityWalPhase::Commit;
        wrong_phase.assign_chain_and_seal(1, None).unwrap();
        assert!(matches!(
            wrong_phase.validate(),
            Err(AuthorityWalContractError::PhasePayloadMismatch { .. })
        ));

        let mut late = record_fixture(AuthorityWalPhase::Commit, &transaction, 1, None);
        let AuthorityWalPayloadV1::Commit(payload) = &mut late.payload else {
            unreachable!()
        };
        payload.committed_at = EXPIRES_AT;
        late.recorded_at = EXPIRES_AT;
        late.assign_chain_and_seal(1, None).unwrap();
        assert!(matches!(
            late.validate_against_transaction(&transaction),
            Err(AuthorityWalContractError::InvalidCommitTime)
        ));
    }

    #[test]
    fn every_versioned_wire_contract_denies_unknown_fields() {
        let positive = positive_fixture();
        let safety = safety_fixture();
        assert_unknown_field_rejected(&positive);
        assert_unknown_field_rejected(&safety);
        assert_unknown_field_rejected(positive.binding());

        let prepare = AuthorityWalPrepareV1 {
            transaction: positive.clone(),
        };
        let provisional = AuthorityWalProvisionalV1 {
            provisional_effects_digest: hash('a'),
        };
        let commit = AuthorityWalCommitV1 {
            committed_at: 1_500,
            protected_time_evidence_digest: hash('b'),
            authorization_snapshot_digest: hash('c'),
            terminal_outcome_digest: hash('d'),
        };
        let abort = AuthorityWalAbortV1 {
            aborted_at: 1_500,
            reason_digest: hash('e'),
            terminal_outcome_digest: hash('f'),
        };
        assert_unknown_field_rejected(&prepare);
        assert_unknown_field_rejected(&provisional);
        assert_unknown_field_rejected(&commit);
        assert_unknown_field_rejected(&abort);
        assert_unknown_field_rejected(&AuthorityWalPayloadV1::Prepare(Box::new(prepare)));
        assert_unknown_field_rejected(&record_fixture(
            AuthorityWalPhase::Prepare,
            &positive,
            1,
            None,
        ));
    }

    #[test]
    fn opaque_signature_bytes_are_excluded_from_self_hash_and_never_authenticated() {
        let transaction = positive_fixture();
        let digest = transaction.compute_transaction_digest().unwrap();
        let mut changed = transaction.clone();
        let AuthorityTransactionV1::PositiveAuthority(positive) = &mut changed else {
            unreachable!()
        };
        positive.signature = signature("different-opaque-bytes");
        assert_eq!(changed.compute_transaction_digest().unwrap(), digest);
        assert_eq!(
            changed.validate().unwrap().integrity,
            AuthorityWalIntegrityDisposition::OpaqueSignaturePresentUnverified
        );

        let record = record_fixture(AuthorityWalPhase::Provisional, &transaction, 1, None);
        let record_digest = record.compute_record_digest().unwrap();
        let mut changed_record_signature = record;
        changed_record_signature.signature = signature("different-record-signature");
        assert_eq!(
            changed_record_signature.compute_record_digest().unwrap(),
            record_digest
        );
    }

    #[test]
    fn exact_phase_and_payload_wire_names_are_stable() {
        let transaction = positive_fixture();
        for (phase, expected) in [
            (AuthorityWalPhase::Prepare, "PREPARE"),
            (AuthorityWalPhase::Provisional, "PROVISIONAL"),
            (AuthorityWalPhase::Commit, "COMMIT"),
            (AuthorityWalPhase::Abort, "ABORT"),
        ] {
            let record = record_fixture(phase, &transaction, 1, None);
            let wire = serde_json::to_value(record).unwrap();
            assert_eq!(wire["phase"], Value::String(expected.to_string()));
            assert_eq!(
                wire["payload"]["payload_kind"],
                Value::String(expected.to_string())
            );
        }
    }
}
