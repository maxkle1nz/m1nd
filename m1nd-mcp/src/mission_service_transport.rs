//! Transport-neutral external facade for the G3 [`MissionService`].
//!
//! The facade is deliberately smaller than the service API. External callers
//! submit typed intents, results, snapshots, and expected canonical bindings;
//! they never submit a mission letter or receipt to persist. Authentication is
//! injected separately by the owner transport after G2 verification, and time
//! is supplied by the owner rather than accepted from the request body.
//!
//! REST and Streamable-HTTP MCP ingress both terminate on this facade. Their
//! status mapping, body limits, and session routing stay transport-owned; the
//! injected G2 authority provider remains the only source of sovereign identity.
//! Neither ingress gains a raw mission-letter or receipt-write primitive.

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use m1nd_control::{
    digest_canonical, verify_canonical_authority_payload_signature, Ingress, MissionState,
    MissionTransitionIntentV1, VerificationKeyRegistryV1,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::authority_runtime::AuthorityRuntimeStatusV1;
use crate::authority_wal::{
    AuthorityWalCryptoAssurance, AuthorityWalRecordCrypto, SoftwareTestAuthorityWalRecordCrypto,
};
use crate::evidence_spine::{
    EvidenceAppendDisposition, EvidenceCorrelationLinkV1, EvidenceSpineError,
    EvidenceSpineIdentityV1, EvidenceSpineStore,
};
use crate::execution_dispatch::RunnerInboxEntryV1;
use crate::mission_service::{
    refuse_external_legacy_mutation, AuthenticatedAuthorityContextV1,
    AuthorityWalCommitCoordinator, LandIntentCoreV1, LandOutcomeV1, LandRequestV1,
    LegacyMutationIngress, MissionService, MissionServiceConfigV1, MissionServiceError,
    MissionTransitionEvidenceV1, MissionTransitionPayloadV1, TransitionOutcomeV1,
};
use crate::owner_authorization_broker::{
    AuthorizationReservationV1, OwnerAuthorityLinearizationV1, OwnerAuthorizationBrokerConfigV1,
    OwnerAuthorizationBrokerError, OwnerAuthorizationBrokerV1,
};
use crate::protected_journal_head::{
    ProtectedJournalHeadAssuranceV1, SharedProtectedJournalHeadBackendV1,
};

pub const MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA: &str =
    "m1nd-mission-service-transport-request-v1";
pub const MISSION_SERVICE_TRANSPORT_RESPONSE_SCHEMA: &str =
    "m1nd-mission-service-transport-response-v1";
pub const MISSION_SERVICE_TRANSPORT_REFUSAL_SCHEMA: &str =
    "m1nd-mission-service-transport-refusal-v1";
pub const LAND_INTENT_READ_AUTHORITY_OBJECT_SCHEMA: &str =
    "m1nd-land-intent-read-authority-object-v1";
pub const LAND_INTENT_READ_AUTHORITY_OBJECT_DIGEST_DOMAIN: &str =
    "m1nd-land-intent-read-authority-object-v1";

/// Real ingress identity supplied by the concrete wire. This is routing and
/// correlation context only; it is never sovereign authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionServiceIngressV1 {
    Rest,
    McpStreamableHttp,
}

/// Owner-observed routing and correlation facts. They bind a ceremony/lease to
/// one ingress context but do not authenticate a subject: REST header values
/// may be caller-supplied labels, while sovereign identity comes only from the
/// signed G2 capability and owner-pinned key registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissionServiceTransportContextV1 {
    pub ingress: MissionServiceIngressV1,
    pub transport_session_id: Option<String>,
    pub ingress_context_digest: Option<String>,
    pub authority_lease_id: Option<String>,
    pub caller_root: Option<String>,
    /// Canonical root used only to discover and prove which existing owner
    /// brain the transport selected. It is not a durable actor identity.
    pub route_selector: Option<String>,
    /// Exact id reported by the selected `BrainActorHandle`. Authority
    /// receipts, leases, journals, previews, and actor jobs bind this fact.
    pub actor_brain_id: Option<String>,
}

/// Closed external request surface. Authority and owner time are intentionally
/// absent: the concrete transport must inject both from trusted owner state.
/// `request_id` is correlation metadata only; mutation idempotency remains the
/// signed inner intent/request key validated by MissionService.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExternalMissionServiceRequestV1 {
    /// Read-only owner composition of the exact land intent that a later
    /// authority transaction must bind.
    LandIntent {
        schema: String,
        request_id: String,
        mission_id: String,
        expected_head_id: String,
        candidate_id: String,
        expected_candidate_digest: String,
        expected_store_version: u64,
        idempotency_key: String,
    },
    /// Non-execution state transition. Execution dispatch/ACK/result transitions
    /// have dedicated variants so runner snapshots cannot be bypassed.
    MissionTransition {
        schema: String,
        request_id: String,
        intent: MissionTransitionIntentV1,
        payload: MissionTransitionPayloadV1,
    },
    ExecutionDispatch {
        schema: String,
        request_id: String,
        intent: MissionTransitionIntentV1,
        payload: MissionTransitionPayloadV1,
    },
    ExecutionStarted {
        schema: String,
        request_id: String,
        snapshot: RunnerInboxEntryV1,
        intent: MissionTransitionIntentV1,
        payload: MissionTransitionPayloadV1,
    },
    ExecutionTerminal {
        schema: String,
        request_id: String,
        snapshot: RunnerInboxEntryV1,
        intent: MissionTransitionIntentV1,
        payload: MissionTransitionPayloadV1,
    },
    Land {
        schema: String,
        request_id: String,
        request: LandRequestV1,
    },
}

impl ExternalMissionServiceRequestV1 {
    pub fn schema(&self) -> &str {
        match self {
            Self::LandIntent { schema, .. }
            | Self::MissionTransition { schema, .. }
            | Self::ExecutionDispatch { schema, .. }
            | Self::ExecutionStarted { schema, .. }
            | Self::ExecutionTerminal { schema, .. }
            | Self::Land { schema, .. } => schema,
        }
    }

    pub fn request_id(&self) -> &str {
        match self {
            Self::LandIntent { request_id, .. }
            | Self::MissionTransition { request_id, .. }
            | Self::ExecutionDispatch { request_id, .. }
            | Self::ExecutionStarted { request_id, .. }
            | Self::ExecutionTerminal { request_id, .. }
            | Self::Land { request_id, .. } => request_id,
        }
    }

    pub const fn action_id(&self) -> &'static str {
        match self {
            Self::LandIntent { .. } => "land_intent",
            Self::MissionTransition { .. } => "mission_transition",
            Self::ExecutionDispatch { .. } => "execution_dispatch",
            Self::ExecutionStarted { .. } => "execution_started",
            Self::ExecutionTerminal { .. } => "execution_terminal",
            Self::Land { .. } => "land",
        }
    }

    pub const fn semantic_action_id(&self) -> &'static str {
        match self {
            Self::LandIntent { .. } => "mission.service.land_intent",
            Self::MissionTransition { .. } => "mission.service.mission_transition",
            Self::ExecutionDispatch { .. } => "mission.service.execution_dispatch",
            Self::ExecutionStarted { .. } => "mission.service.execution_started",
            Self::ExecutionTerminal { .. } => "mission.service.execution_terminal",
            Self::Land { .. } => "mission.service.land",
        }
    }

    /// Exact semantic object digest that upstream authorization must bind.
    /// The read-only LandIntent path is authority-bearing too: its canonical
    /// digest includes every equality input and excludes only `request_id`,
    /// which is declared correlation metadata rather than semantic input.
    pub fn authority_object_digest(&self) -> Result<String, MissionServiceTransportError> {
        match self {
            Self::LandIntent {
                mission_id,
                expected_head_id,
                candidate_id,
                expected_candidate_digest,
                expected_store_version,
                idempotency_key,
                ..
            } => digest_canonical(
                LAND_INTENT_READ_AUTHORITY_OBJECT_DIGEST_DOMAIN,
                &LandIntentReadAuthorityObjectV1 {
                    schema: LAND_INTENT_READ_AUTHORITY_OBJECT_SCHEMA,
                    action: self.semantic_action_id(),
                    mission_id,
                    expected_head_id,
                    candidate_id,
                    expected_candidate_digest,
                    expected_store_version: *expected_store_version,
                    idempotency_key,
                },
            )
            .map_err(|error| {
                MissionServiceTransportError::refused(
                    "authority_object_digest_failed",
                    error.to_string(),
                )
            }),
            Self::MissionTransition { intent, .. }
            | Self::ExecutionDispatch { intent, .. }
            | Self::ExecutionStarted { intent, .. }
            | Self::ExecutionTerminal { intent, .. } => Ok(intent.intent_digest.clone()),
            Self::Land { request, .. } => Ok(request.transaction.transaction_digest().to_string()),
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct LandIntentReadAuthorityObjectV1<'a> {
    schema: &'static str,
    action: &'static str,
    mission_id: &'a str,
    expected_head_id: &'a str,
    candidate_id: &'a str,
    expected_candidate_digest: &'a str,
    expected_store_version: u64,
    idempotency_key: &'a str,
}

/// Narrow G2 seam. Implementations must return only contexts produced by a
/// successful upstream verifier. The local HTTP bearer is explicitly not such
/// a context. Returning `None` is the normal fail-closed production posture
/// while no sovereign provider is installed.
pub trait MissionServiceAuthorityProvider: Send + Sync {
    fn authenticated_authority(
        &self,
        context: &MissionServiceTransportContextV1,
        request: &ExternalMissionServiceRequestV1,
        verified_object_digest: &str,
        owner_now_ms: u64,
    ) -> Result<Option<AuthenticatedAuthorityContextV1>, MissionServiceTransportError>;

    /// Execute after reservation. The default preserves the explicit test seam;
    /// the production owner-broker implementation consumes a non-WAL lease
    /// before dispatch or coordinates the exact WAL COMMIT for landing.
    fn dispatch_reserved(
        &self,
        service: &mut MissionService,
        authority: Option<&AuthenticatedAuthorityContextV1>,
        request: &ExternalMissionServiceRequestV1,
        owner_now_ms: u64,
    ) -> Result<MissionServiceTransportResponseV1, MissionServiceTransportError> {
        dispatch_external_mission_request_with_coordinator(
            service,
            authority,
            request,
            owner_now_ms,
            None,
        )
    }
}

impl<F> MissionServiceAuthorityProvider for F
where
    F: Fn(
            &MissionServiceTransportContextV1,
            &ExternalMissionServiceRequestV1,
            &str,
            u64,
        ) -> Result<Option<AuthenticatedAuthorityContextV1>, MissionServiceTransportError>
        + Send
        + Sync,
{
    fn authenticated_authority(
        &self,
        context: &MissionServiceTransportContextV1,
        request: &ExternalMissionServiceRequestV1,
        verified_object_digest: &str,
        owner_now_ms: u64,
    ) -> Result<Option<AuthenticatedAuthorityContextV1>, MissionServiceTransportError> {
        self(context, request, verified_object_digest, owner_now_ms)
    }
}

pub type AuthorityStatusReader =
    dyn Fn() -> Result<AuthorityRuntimeStatusV1, String> + Send + Sync + 'static;

/// Production-shaped owner adapter. It is intentionally constructed from one
/// durable broker and one current-status reader; local bearer authentication is
/// never accepted as sovereign authority.
pub struct OwnerBrokerMissionServiceAuthorityProviderV1 {
    broker_config: OwnerAuthorizationBrokerConfigV1,
    linearization: OwnerAuthorityLinearizationV1,
    broker_operation: Arc<Mutex<()>>,
    current_authority: Arc<AuthorityStatusReader>,
    protected_journal_head: Option<SharedProtectedJournalHeadBackendV1>,
    transaction_verification_keys: Option<Arc<VerificationKeyRegistryV1>>,
    max_future_clock_skew_ms: u64,
    receipt_crypto: Option<Arc<dyn AuthorityWalRecordCrypto>>,
}

pub(crate) struct OwnerBrokerAuthorityProviderInputsV1 {
    pub broker_config: OwnerAuthorizationBrokerConfigV1,
    pub linearization: OwnerAuthorityLinearizationV1,
    pub broker_operation: Arc<Mutex<()>>,
    pub current_authority: Arc<AuthorityStatusReader>,
    pub protected_journal_head: SharedProtectedJournalHeadBackendV1,
    pub transaction_verification_keys: Arc<VerificationKeyRegistryV1>,
    pub max_future_clock_skew_ms: u64,
    pub receipt_crypto: Arc<dyn AuthorityWalRecordCrypto>,
}

struct PinnedArtifactSignatureInputV1<'a> {
    issuer: &'a str,
    key_id: &'a str,
    algorithm: &'a str,
    signature: &'a m1nd_control::OpaqueSignature,
    domain: &'static str,
    canonical: &'a [u8],
    owner_now_ms: u64,
    artifact: &'static str,
}

impl OwnerBrokerMissionServiceAuthorityProviderV1 {
    pub(crate) fn new(
        broker_config: OwnerAuthorizationBrokerConfigV1,
        linearization: OwnerAuthorityLinearizationV1,
        current_authority: Arc<AuthorityStatusReader>,
    ) -> Self {
        Self {
            broker_config,
            linearization,
            broker_operation: Arc::new(Mutex::new(())),
            current_authority,
            protected_journal_head: None,
            transaction_verification_keys: None,
            max_future_clock_skew_ms: 0,
            receipt_crypto: None,
        }
    }

    pub(crate) fn from_owner_inputs(inputs: OwnerBrokerAuthorityProviderInputsV1) -> Self {
        Self {
            broker_config: inputs.broker_config,
            linearization: inputs.linearization,
            broker_operation: inputs.broker_operation,
            current_authority: inputs.current_authority,
            protected_journal_head: Some(inputs.protected_journal_head),
            transaction_verification_keys: Some(inputs.transaction_verification_keys),
            max_future_clock_skew_ms: inputs.max_future_clock_skew_ms,
            receipt_crypto: Some(inputs.receipt_crypto),
        }
    }

    fn open_broker(&self) -> Result<OwnerAuthorizationBrokerV1, OwnerAuthorizationBrokerError> {
        match &self.protected_journal_head {
            Some(protected_head) => OwnerAuthorizationBrokerV1::open_with_protected_head(
                self.broker_config.clone(),
                self.linearization.clone(),
                Arc::clone(protected_head),
            ),
            None => OwnerAuthorizationBrokerV1::open(
                self.broker_config.clone(),
                self.linearization.clone(),
            ),
        }
    }

    fn reservation_for(
        &self,
        authority: &AuthenticatedAuthorityContextV1,
    ) -> Result<AuthorizationReservationV1, MissionServiceTransportError> {
        let _operation = self.broker_operation.lock();
        let broker = self.open_broker()?;
        let lease = broker
            .lease(&authority.authorization_lease_id)
            .ok_or_else(|| {
                MissionServiceTransportError::refused(
                    "authorization_lease_not_found",
                    &authority.authorization_lease_id,
                )
            })?;
        let reservation = lease.reservation.clone().ok_or_else(|| {
            MissionServiceTransportError::refused(
                "authorization_reservation_not_current",
                "lease has no active reservation",
            )
        })?;
        if reservation.reservation_id != authority.authorization_reservation_id {
            return Err(MissionServiceTransportError::refused(
                "authorization_reservation_not_current",
                "authority context does not bind the current reservation",
            ));
        }
        Ok(reservation)
    }

    fn verify_outer_transaction(
        &self,
        transaction: &m1nd_control::AuthorityTransactionV1,
        owner_now_ms: u64,
    ) -> Result<(), MissionServiceTransportError> {
        transaction.validate().map_err(|error| {
            MissionServiceTransportError::refused(
                "outer_authority_transaction_invalid",
                error.to_string(),
            )
        })?;
        let keys = self.transaction_verification_keys.as_ref().ok_or_else(|| {
            MissionServiceTransportError::refused(
                "outer_authority_transaction_verifier_not_installed",
                "production transaction verifier is NOT_INSTALLED",
            )
        })?;
        let (issuer, key_id, algorithm, signature) = match transaction {
            m1nd_control::AuthorityTransactionV1::PositiveAuthority(transaction) => (
                transaction.issuer.as_str(),
                transaction.key_id.as_str(),
                transaction.algorithm.as_str(),
                &transaction.signature,
            ),
            m1nd_control::AuthorityTransactionV1::SafetyKernel(transaction) => (
                transaction.issuer.as_str(),
                transaction.key_id.as_str(),
                transaction.algorithm.as_str(),
                &transaction.signature,
            ),
        };
        if issuer != transaction.binding().subject_id {
            return Err(MissionServiceTransportError::refused(
                "outer_authority_transaction_identity_mismatch",
                "outer transaction issuer differs from its bound subject",
            ));
        }
        let key = keys
            .resolve_active(key_id, issuer, owner_now_ms, self.max_future_clock_skew_ms)
            .map_err(|error| {
                MissionServiceTransportError::refused(
                    "outer_authority_transaction_key_inactive",
                    error.to_string(),
                )
            })?;
        if algorithm != key.algorithm {
            return Err(MissionServiceTransportError::refused(
                "outer_authority_transaction_algorithm_mismatch",
                "outer transaction algorithm differs from the owner-pinned key",
            ));
        }
        let canonical = transaction.canonical_signature_payload().map_err(|error| {
            MissionServiceTransportError::refused(
                "outer_authority_transaction_canonicalization_failed",
                error.to_string(),
            )
        })?;
        verify_canonical_authority_payload_signature(
            m1nd_control::AUTHORITY_TRANSACTION_SIGNATURE_DOMAIN,
            &canonical,
            signature,
            key,
        )
        .map_err(|error| {
            MissionServiceTransportError::refused(
                "outer_authority_transaction_signature_invalid",
                error.to_string(),
            )
        })?;
        Ok(())
    }

    fn verify_signed_transition_evidence(
        &self,
        request: &ExternalMissionServiceRequestV1,
        owner_now_ms: u64,
    ) -> Result<(), MissionServiceTransportError> {
        let payload = match request {
            ExternalMissionServiceRequestV1::MissionTransition { payload, .. }
            | ExternalMissionServiceRequestV1::ExecutionDispatch { payload, .. }
            | ExternalMissionServiceRequestV1::ExecutionStarted { payload, .. }
            | ExternalMissionServiceRequestV1::ExecutionTerminal { payload, .. } => payload,
            ExternalMissionServiceRequestV1::LandIntent { .. }
            | ExternalMissionServiceRequestV1::Land { .. } => return Ok(()),
        };
        match &payload.evidence {
            MissionTransitionEvidenceV1::ExecutionResult { result, .. } => {
                let canonical = result.canonical_signature_payload().map_err(|error| {
                    MissionServiceTransportError::refused(
                        "execution_result_canonicalization_failed",
                        error.to_string(),
                    )
                })?;
                self.verify_pinned_artifact_signature(PinnedArtifactSignatureInputV1 {
                    issuer: result.issuer.as_str(),
                    key_id: result.key_id.as_str(),
                    algorithm: result.algorithm.as_str(),
                    signature: &result.signature,
                    domain: m1nd_control::EXECUTION_RESULT_SIGNATURE_DOMAIN,
                    canonical: &canonical,
                    owner_now_ms,
                    artifact: "execution_result",
                })
            }
            MissionTransitionEvidenceV1::ReviewResult { result, .. } => {
                let canonical = result.canonical_signature_payload().map_err(|error| {
                    MissionServiceTransportError::refused(
                        "review_result_canonicalization_failed",
                        error.to_string(),
                    )
                })?;
                self.verify_pinned_artifact_signature(PinnedArtifactSignatureInputV1 {
                    issuer: result.issuer.as_str(),
                    key_id: result.key_id.as_str(),
                    algorithm: result.algorithm.as_str(),
                    signature: &result.signature,
                    domain: m1nd_control::REVIEW_RESULT_SIGNATURE_DOMAIN,
                    canonical: &canonical,
                    owner_now_ms,
                    artifact: "review_result",
                })
            }
            _ => Ok(()),
        }
    }

    fn verify_pinned_artifact_signature(
        &self,
        input: PinnedArtifactSignatureInputV1<'_>,
    ) -> Result<(), MissionServiceTransportError> {
        let PinnedArtifactSignatureInputV1 {
            issuer,
            key_id,
            algorithm,
            signature,
            domain,
            canonical,
            owner_now_ms,
            artifact,
        } = input;
        let keys = self.transaction_verification_keys.as_ref().ok_or_else(|| {
            MissionServiceTransportError::refused(
                "signed_artifact_verifier_not_installed",
                format!("production verifier for {artifact} is NOT_INSTALLED"),
            )
        })?;
        let key = keys
            .resolve_active(key_id, issuer, owner_now_ms, self.max_future_clock_skew_ms)
            .map_err(|error| {
                MissionServiceTransportError::refused(
                    "signed_artifact_key_inactive",
                    format!("{artifact}: {error}"),
                )
            })?;
        if algorithm != key.algorithm {
            return Err(MissionServiceTransportError::refused(
                "signed_artifact_algorithm_mismatch",
                format!("{artifact} algorithm differs from the owner-pinned key"),
            ));
        }
        verify_canonical_authority_payload_signature(domain, canonical, signature, key).map_err(
            |error| {
                MissionServiceTransportError::refused(
                    "signed_artifact_signature_invalid",
                    format!("{artifact}: {error}"),
                )
            },
        )?;
        Ok(())
    }
}

impl MissionServiceAuthorityProvider for OwnerBrokerMissionServiceAuthorityProviderV1 {
    fn authenticated_authority(
        &self,
        context: &MissionServiceTransportContextV1,
        request: &ExternalMissionServiceRequestV1,
        verified_object_digest: &str,
        owner_now_ms: u64,
    ) -> Result<Option<AuthenticatedAuthorityContextV1>, MissionServiceTransportError> {
        let lease_id = required_transport_fact(
            context.authority_lease_id.as_deref(),
            "missing_authorization_lease",
            "a one-shot owner authorization lease is required",
        )?;
        let transport_session_id = required_transport_fact(
            context.transport_session_id.as_deref(),
            "missing_transport_session",
            "authorization leases require an owner-observed transport correlation label",
        )?;
        let ingress_context_digest = required_transport_fact(
            context.ingress_context_digest.as_deref(),
            "missing_ingress_context_digest",
            "authorization leases require a trusted ingress-context digest",
        )?;
        let brain_id = required_transport_fact(
            context.actor_brain_id.as_deref(),
            "missing_actor_brain_id",
            "authorization leases are bound to the exact selected brain actor",
        )?;
        let expected_ingress = match context.ingress {
            MissionServiceIngressV1::Rest => Ingress::Rest,
            MissionServiceIngressV1::McpStreamableHttp => Ingress::Mcp,
        };
        let _operation = self.broker_operation.lock();
        let mut broker = self.open_broker()?;
        let lease = broker.lease(lease_id).cloned().ok_or_else(|| {
            MissionServiceTransportError::refused("authorization_lease_not_found", lease_id)
        })?;
        let receipt_crypto = self.receipt_crypto.as_ref().ok_or_else(|| {
            MissionServiceTransportError::refused(
                "authorization_receipt_verifier_not_installed",
                "production authorization receipt verifier is NOT_INSTALLED",
            )
        })?;
        crate::authority_transport::verify_authorization_receipt(
            &lease.authorization_receipt,
            receipt_crypto.as_ref(),
        )
        .map_err(|error| MissionServiceTransportError::refused(error.code(), error.to_string()))?;
        let receipt = &lease.authorization_receipt.core;
        let (mission_id, mission_head_id) = request_mission_binding(request);
        if receipt.brain_id != brain_id
            || receipt.action.as_str() != request.semantic_action_id()
            || receipt.ingress != expected_ingress
            || receipt.mission_id.as_deref() != mission_id
            || receipt.mission_head_id.as_deref() != mission_head_id
        {
            return Err(MissionServiceTransportError::refused(
                "authorization_request_binding_mismatch",
                "lease receipt differs from request action, ingress, brain, mission, or head",
            ));
        }
        self.verify_signed_transition_evidence(request, owner_now_ms)?;
        let (reservation, identity_role_binding_digest) = match request {
            ExternalMissionServiceRequestV1::Land { request, .. } => {
                self.verify_outer_transaction(&request.transaction, owner_now_ms)?;
                let identity = match &request.transaction {
                    m1nd_control::AuthorityTransactionV1::PositiveAuthority(transaction) => {
                        Some(transaction.identity_role_binding_digest.clone())
                    }
                    m1nd_control::AuthorityTransactionV1::SafetyKernel(_) => None,
                };
                (
                    broker.reserve_land(
                        lease_id,
                        transport_session_id,
                        ingress_context_digest,
                        &request.transaction,
                        owner_now_ms,
                    )?,
                    identity,
                )
            }
            _ => (
                broker.reserve(
                    lease_id,
                    transport_session_id,
                    ingress_context_digest,
                    verified_object_digest,
                    owner_now_ms,
                )?,
                None,
            ),
        };
        Ok(Some(broker.mission_service_context(
            &reservation,
            identity_role_binding_digest,
        )?))
    }

    fn dispatch_reserved(
        &self,
        service: &mut MissionService,
        authority: Option<&AuthenticatedAuthorityContextV1>,
        request: &ExternalMissionServiceRequestV1,
        owner_now_ms: u64,
    ) -> Result<MissionServiceTransportResponseV1, MissionServiceTransportError> {
        let authority = require_authority(authority)?;
        let reservation = self.reservation_for(authority)?;
        if matches!(request, ExternalMissionServiceRequestV1::Land { .. }) {
            let coordinator = BrokerAuthorityWalCommitCoordinatorV1 {
                broker_config: self.broker_config.clone(),
                linearization: self.linearization.clone(),
                broker_operation: Arc::clone(&self.broker_operation),
                current_authority: Arc::clone(&self.current_authority),
                protected_journal_head: self.protected_journal_head.clone(),
                reservation,
            };
            return dispatch_external_mission_request_with_coordinator(
                service,
                Some(authority),
                request,
                owner_now_ms,
                Some(&coordinator),
            );
        }

        // Holding the broker owner mutex through dispatch is the non-WAL
        // linearization boundary. All production authority changes use this
        // same coordinator before touching AuthorityRuntime.
        let _operation = self.broker_operation.lock();
        let mut broker = self.open_broker()?;
        let current = (self.current_authority)().map_err(|detail| {
            MissionServiceTransportError::refused("authority_runtime_unavailable", detail)
        })?;
        broker.admit_non_wal(&reservation, &current, owner_now_ms)?;
        dispatch_external_mission_request_with_coordinator(
            service,
            Some(authority),
            request,
            owner_now_ms,
            None,
        )
    }
}

struct BrokerAuthorityWalCommitCoordinatorV1 {
    broker_config: OwnerAuthorizationBrokerConfigV1,
    linearization: OwnerAuthorityLinearizationV1,
    broker_operation: Arc<Mutex<()>>,
    current_authority: Arc<AuthorityStatusReader>,
    protected_journal_head: Option<SharedProtectedJournalHeadBackendV1>,
    reservation: AuthorizationReservationV1,
}

impl AuthorityWalCommitCoordinator for BrokerAuthorityWalCommitCoordinatorV1 {
    fn append_commit(
        &self,
        _authority: &AuthenticatedAuthorityContextV1,
        _transaction: &m1nd_control::AuthorityTransactionV1,
        committed_at: u64,
        append: &mut dyn FnMut() -> Result<
            crate::authority_wal::AuthorityWalAppendOutcome,
            MissionServiceError,
        >,
    ) -> Result<crate::authority_wal::AuthorityWalAppendOutcome, MissionServiceError> {
        let _operation = self.broker_operation.lock();
        let mut broker = match &self.protected_journal_head {
            Some(protected_head) => OwnerAuthorizationBrokerV1::open_with_protected_head(
                self.broker_config.clone(),
                self.linearization.clone(),
                Arc::clone(protected_head),
            ),
            None => OwnerAuthorizationBrokerV1::open(
                self.broker_config.clone(),
                self.linearization.clone(),
            ),
        }
        .map_err(broker_error_as_mission_service)?;
        let current = (self.current_authority)().map_err(|detail| {
            MissionServiceError::refused("authority_runtime_unavailable", detail)
        })?;
        let mut append_outcome = None;
        broker
            .finalize_wal(&self.reservation, &current, committed_at, || {
                let outcome = append().map_err(|error| error.to_string())?;
                let verified_witness = match &outcome {
                    crate::authority_wal::AuthorityWalAppendOutcome::Appended {
                        phase,
                        verified_commit_witness: Some(witness),
                        ..
                    } if *phase == m1nd_control::AuthorityWalPhase::Commit => witness.clone(),
                    crate::authority_wal::AuthorityWalAppendOutcome::TerminalReplay(terminal)
                        if terminal.phase == m1nd_control::AuthorityWalPhase::Commit =>
                    {
                        terminal.verified_commit_witness().ok_or_else(|| {
                            "verified COMMIT replay omitted its opaque WAL witness".to_string()
                        })?
                    }
                    _ => return Err("commit coordinator observed a non-COMMIT outcome".to_string()),
                };
                append_outcome = Some(outcome);
                Ok(verified_witness)
            })
            .map_err(broker_error_as_mission_service)?;
        append_outcome.ok_or_else(|| MissionServiceError::Corruption {
            detail: "broker consumed WAL authorization without an append outcome".to_string(),
        })
    }
}

fn required_transport_fact<'a>(
    value: Option<&'a str>,
    code: &'static str,
    detail: &'static str,
) -> Result<&'a str, MissionServiceTransportError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| MissionServiceTransportError::refused(code, detail))
}

fn request_mission_binding(
    request: &ExternalMissionServiceRequestV1,
) -> (Option<&str>, Option<&str>) {
    match request {
        ExternalMissionServiceRequestV1::LandIntent {
            mission_id,
            expected_head_id,
            ..
        } => (Some(mission_id), Some(expected_head_id)),
        ExternalMissionServiceRequestV1::MissionTransition { intent, .. }
        | ExternalMissionServiceRequestV1::ExecutionDispatch { intent, .. }
        | ExternalMissionServiceRequestV1::ExecutionStarted { intent, .. }
        | ExternalMissionServiceRequestV1::ExecutionTerminal { intent, .. } => {
            (Some(&intent.mission_id), intent.expected_head_id.as_deref())
        }
        ExternalMissionServiceRequestV1::Land { request, .. } => {
            (Some(&request.mission_id), Some(&request.expected_head_id))
        }
    }
}

fn broker_error_as_mission_service(error: OwnerAuthorizationBrokerError) -> MissionServiceError {
    MissionServiceError::refused(error.code(), error.to_string())
}

/// Thread-safe concrete facade shared by REST and Streamable-HTTP MCP. It owns
/// the one durable MissionService instance and obtains authority only through
/// the injected G2 seam above.
pub struct MissionServiceTransportFacade {
    root: PathBuf,
    config: MissionServiceConfigV1,
    operation_lock: Mutex<()>,
    authority_provider: Arc<dyn MissionServiceAuthorityProvider>,
    owner_clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    wal_record_crypto: Option<Arc<dyn AuthorityWalRecordCrypto>>,
    protected_journal_head: Option<SharedProtectedJournalHeadBackendV1>,
    evidence_projection: Option<MissionServiceEvidenceProjectionV1>,
}

#[derive(Clone, Debug)]
struct MissionServiceEvidenceProjectionV1 {
    root: PathBuf,
    workspace_root: PathBuf,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct EvidenceSyncSummaryV1 {
    appended: u64,
    replayed: u64,
}

impl EvidenceSyncSummaryV1 {
    fn observe(&mut self, disposition: EvidenceAppendDisposition) {
        match disposition {
            EvidenceAppendDisposition::Appended => self.appended += 1,
            EvidenceAppendDisposition::Replayed => self.replayed += 1,
        }
    }
}

impl MissionServiceTransportFacade {
    pub fn open(
        root: impl AsRef<Path>,
        config: MissionServiceConfigV1,
        authority_provider: Arc<dyn MissionServiceAuthorityProvider>,
    ) -> Result<Self, MissionServiceTransportError> {
        Self::open_with_clock(
            root,
            config,
            authority_provider,
            Arc::new(crate::util::now_ms),
        )
    }

    pub fn open_with_clock(
        root: impl AsRef<Path>,
        config: MissionServiceConfigV1,
        authority_provider: Arc<dyn MissionServiceAuthorityProvider>,
        owner_clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> Result<Self, MissionServiceTransportError> {
        Self::open_internal(
            root,
            config,
            authority_provider,
            owner_clock,
            None,
            None,
            None,
        )
    }

    /// Production constructor. A software-test signer is rejected even when a
    /// caller attempts to inject it explicitly; production serving therefore
    /// cannot silently downgrade WAL record authenticity.
    pub fn open_with_production_wal_crypto(
        root: impl AsRef<Path>,
        config: MissionServiceConfigV1,
        authority_provider: Arc<dyn MissionServiceAuthorityProvider>,
        owner_clock: Arc<dyn Fn() -> u64 + Send + Sync>,
        wal_record_crypto: Arc<dyn AuthorityWalRecordCrypto>,
        protected_journal_head: SharedProtectedJournalHeadBackendV1,
    ) -> Result<Self, MissionServiceTransportError> {
        if wal_record_crypto.assurance() != AuthorityWalCryptoAssurance::ProductionCryptographic {
            return Err(MissionServiceTransportError::refused(
                "production_wal_crypto_required",
                "production facade requires a cryptographic AuthorityWAL signer/verifier",
            ));
        }
        if protected_journal_head.lock().assurance()
            != ProtectedJournalHeadAssuranceV1::HardwareProtectedAttested
        {
            return Err(MissionServiceTransportError::refused(
                "production_wal_protected_head_required",
                "production facade requires a hardware-protected AuthorityWAL head",
            ));
        }
        Self::open_internal(
            root,
            config,
            authority_provider,
            owner_clock,
            None,
            Some(wal_record_crypto),
            Some(protected_journal_head),
        )
    }

    /// Explicit test-only facade. It never selects this software signer from
    /// environment or production configuration implicitly.
    pub fn open_with_clock_software_test_not_production(
        root: impl AsRef<Path>,
        config: MissionServiceConfigV1,
        authority_provider: Arc<dyn MissionServiceAuthorityProvider>,
        owner_clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> Result<Self, MissionServiceTransportError> {
        Self::open_internal(
            root,
            config,
            authority_provider,
            owner_clock,
            None,
            Some(Arc::new(
                SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(
                    b"m1nd-authority-wal-explicit-test-secret-v1",
                ),
            )),
            None,
        )
    }

    /// Optional G5 constructor. It does not widen the G2 authority seam: the
    /// projector observes only canonical MissionService state after the service
    /// has accepted/recovered it, and writes into an owner-selected workspace.
    pub fn open_with_clock_and_evidence_spine(
        root: impl AsRef<Path>,
        config: MissionServiceConfigV1,
        authority_provider: Arc<dyn MissionServiceAuthorityProvider>,
        owner_clock: Arc<dyn Fn() -> u64 + Send + Sync>,
        evidence_spine_root: impl AsRef<Path>,
        workspace_root: impl AsRef<Path>,
    ) -> Result<Self, MissionServiceTransportError> {
        Self::open_internal(
            root,
            config,
            authority_provider,
            owner_clock,
            Some(MissionServiceEvidenceProjectionV1 {
                root: evidence_spine_root.as_ref().to_path_buf(),
                workspace_root: workspace_root.as_ref().to_path_buf(),
            }),
            None,
            None,
        )
    }

    fn open_internal(
        root: impl AsRef<Path>,
        config: MissionServiceConfigV1,
        authority_provider: Arc<dyn MissionServiceAuthorityProvider>,
        owner_clock: Arc<dyn Fn() -> u64 + Send + Sync>,
        evidence_projection: Option<MissionServiceEvidenceProjectionV1>,
        wal_record_crypto: Option<Arc<dyn AuthorityWalRecordCrypto>>,
        protected_journal_head: Option<SharedProtectedJournalHeadBackendV1>,
    ) -> Result<Self, MissionServiceTransportError> {
        let root = root.as_ref().to_path_buf();
        // Validate identity/config and execute recovery once before publishing
        // the facade. The non-Send WAL guard is born and dropped on this thread.
        let service = match (wal_record_crypto.as_ref(), protected_journal_head.as_ref()) {
            (Some(crypto), Some(protected_head)) => {
                MissionService::open_with_wal_record_crypto_and_protected_head(
                    &root,
                    config.clone(),
                    Arc::clone(crypto),
                    Arc::clone(protected_head),
                )?
            }
            (Some(crypto), None) => MissionService::open_with_wal_record_crypto(
                &root,
                config.clone(),
                Arc::clone(crypto),
            )?,
            (None, None) => MissionService::open(&root, config.clone())?,
            (None, Some(_)) => {
                return Err(MissionServiceTransportError::refused(
                    "authority_wal_crypto_required",
                    "protected WAL heads require explicit record crypto",
                ));
            }
        };
        if let Some(projection) = &evidence_projection {
            sync_mission_service_evidence(&service, &config, projection, (owner_clock)())?;
        }
        drop(service);
        Ok(Self {
            root,
            config,
            operation_lock: Mutex::new(()),
            authority_provider,
            owner_clock,
            wal_record_crypto,
            protected_journal_head,
            evidence_projection,
        })
    }

    /// Concrete wire entry: owner time comes from the injected owner clock,
    /// never from HTTP/MCP JSON.
    pub fn dispatch_wire_json(
        &self,
        context: &MissionServiceTransportContextV1,
        body: &[u8],
    ) -> Result<MissionServiceTransportResponseV1, MissionServiceTransportError> {
        self.dispatch_json(context, body, (self.owner_clock)())
    }

    /// Dispatch one real-wire JSON body. Legacy action names are refused after
    /// the minimal action probe and before strict request decoding or authority
    /// provider invocation.
    pub fn dispatch_json(
        &self,
        context: &MissionServiceTransportContextV1,
        body: &[u8],
        owner_now_ms: u64,
    ) -> Result<MissionServiceTransportResponseV1, MissionServiceTransportError> {
        let probe: ExternalActionProbe = serde_json::from_slice(body)?;
        if let Some(error) = legacy_mutation_error(&probe.action) {
            return Err(error);
        }
        let request: ExternalMissionServiceRequestV1 = serde_json::from_slice(body)?;
        validate_metadata(&request)?;
        let _operation = self.operation_lock.lock();
        let authority_object_digest = request.authority_object_digest()?;
        let authority = self.authority_provider.authenticated_authority(
            context,
            &request,
            &authority_object_digest,
            owner_now_ms,
        )?;
        let mut service = self.open_service()?;
        let mut response = self.authority_provider.dispatch_reserved(
            &mut service,
            authority.as_ref(),
            &request,
            owner_now_ms,
        )?;
        response.evidence_projection =
            Some(self.project_response(&service, &request, &response.result, owner_now_ms));
        Ok(response)
    }

    pub fn state_version(&self) -> Result<u64, MissionServiceTransportError> {
        let _operation = self.operation_lock.lock();
        let service = self.open_service()?;
        Ok(service.state_version())
    }

    fn open_service(&self) -> Result<MissionService, MissionServiceTransportError> {
        match (
            self.wal_record_crypto.as_ref(),
            self.protected_journal_head.as_ref(),
        ) {
            (Some(crypto), Some(protected_head)) => Ok(
                MissionService::open_with_wal_record_crypto_and_protected_head(
                    &self.root,
                    self.config.clone(),
                    Arc::clone(crypto),
                    Arc::clone(protected_head),
                )?,
            ),
            (Some(crypto), None) => Ok(MissionService::open_with_wal_record_crypto(
                &self.root,
                self.config.clone(),
                Arc::clone(crypto),
            )?),
            (None, None) => Ok(MissionService::open(&self.root, self.config.clone())?),
            (None, Some(_)) => Err(MissionServiceTransportError::refused(
                "authority_wal_crypto_required",
                "protected WAL heads require explicit record crypto",
            )),
        }
    }

    fn project_response(
        &self,
        service: &MissionService,
        request: &ExternalMissionServiceRequestV1,
        result: &MissionServiceTransportResultV1,
        observed_at: u64,
    ) -> Value {
        let Some(projection) = &self.evidence_projection else {
            return projection_gap(
                "evidence_spine_not_configured",
                "MissionService committed its own canonical result, but this facade has no optional G5 projector installed",
            );
        };
        match sync_mission_service_evidence(service, &self.config, projection, observed_at) {
            Ok(summary) => {
                let link = correlation_link_for_result(service, request, result);
                match link {
                    Ok(link) => json!({
                        "schema": crate::evidence_spine_owner::EVIDENCE_PROJECTION_STATUS_SCHEMA,
                        "status": "synchronized",
                        "appended": summary.appended,
                        "replayed": summary.replayed,
                        "evidence_link": link,
                    }),
                    Err(detail) => projection_gap("canonical_link_unavailable", detail),
                }
            }
            Err(error) => projection_gap(
                "evidence_projection_failed_after_authority_result",
                format!(
                    "MissionService result remains committed in its authority store; projection will be retried on facade restart or the next request: {error}"
                ),
            ),
        }
    }
}

/// Synchronize only canonical state already recovered by MissionService. This
/// projection is idempotent: reopening the facade replays existing G5 rows and
/// appends only authority events that were not projected before a crash.
fn sync_mission_service_evidence(
    service: &MissionService,
    config: &MissionServiceConfigV1,
    projection: &MissionServiceEvidenceProjectionV1,
    observed_at: u64,
) -> Result<EvidenceSyncSummaryV1, EvidenceSpineError> {
    let identity = EvidenceSpineIdentityV1::new(
        config.organism_id.clone(),
        config.brain_id.clone(),
        &projection.workspace_root,
    )?;
    let mut store = EvidenceSpineStore::open(&projection.root, identity)?;
    let mut summary = EvidenceSyncSummaryV1::default();

    for letter in service.letters() {
        summary.observe(
            store
                .record_mission_letter(letter, observed_at)?
                .disposition,
        );
    }
    for receipt in service.receipts() {
        summary.observe(store.record_receipt(receipt, observed_at)?.disposition);
    }

    Ok(summary)
}

/// Derive the non-authoritative correlation token from the exact canonical G3
/// result. The caller can carry the token, but G5 will revalidate its existing
/// Receipt/MissionLetter anchor before accepting any coordination projection.
fn correlation_link_for_result(
    service: &MissionService,
    request: &ExternalMissionServiceRequestV1,
    result: &MissionServiceTransportResultV1,
) -> Result<EvidenceCorrelationLinkV1, String> {
    match (request, result) {
        (
            ExternalMissionServiceRequestV1::MissionTransition { .. }
            | ExternalMissionServiceRequestV1::ExecutionDispatch { .. }
            | ExternalMissionServiceRequestV1::ExecutionStarted { .. }
            | ExternalMissionServiceRequestV1::ExecutionTerminal { .. },
            MissionServiceTransportResultV1::MissionTransition { outcome },
        ) => EvidenceCorrelationLinkV1::from_letter(&outcome.letter)
            .map_err(|error| error.to_string()),
        (
            ExternalMissionServiceRequestV1::Land {
                request: land_request,
                ..
            },
            MissionServiceTransportResultV1::Land { outcome },
        ) => {
            let letter = service.head(&land_request.mission_id).ok_or_else(|| {
                format!(
                    "canonical landed head for mission '{}' is absent",
                    land_request.mission_id
                )
            })?;
            if letter.head_id != outcome.letter_id
                || letter.transaction_id.as_deref() != Some(outcome.transaction_id.as_str())
                || letter.committed_receipt_id.as_deref() != Some(outcome.receipt_id.as_str())
            {
                return Err(
                    "land outcome does not match the canonical MissionService head".to_string(),
                );
            }
            let receipt = service
                .receipt(&outcome.receipt_id)
                .ok_or_else(|| format!("canonical receipt '{}' is absent", outcome.receipt_id))?;
            if receipt.receipt_digest != outcome.receipt_digest
                || receipt.transaction_id != outcome.transaction_id
                || receipt.mission_id != land_request.mission_id
                || letter.previous_head_id.as_deref() != Some(receipt.mission_head_id.as_str())
            {
                return Err(
                    "land outcome does not match its canonical ReceiptV1 anchor".to_string()
                );
            }
            EvidenceCorrelationLinkV1::from_letter(letter).map_err(|error| error.to_string())
        }
        (
            ExternalMissionServiceRequestV1::LandIntent { .. },
            MissionServiceTransportResultV1::LandIntent { .. },
        ) => Err(
            "land_intent is read-only and emits no canonical ReceiptV1 or MissionLetterV1 anchor"
                .to_string(),
        ),
        _ => Err("transport request/result variants do not match".to_string()),
    }
}

fn projection_gap(code: &str, detail: impl Into<String>) -> Value {
    crate::evidence_spine_owner::gap_status(code, detail)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MissionServiceTransportResultV1 {
    LandIntent { intent: Box<LandIntentCoreV1> },
    MissionTransition { outcome: Box<TransitionOutcomeV1> },
    Land { outcome: Box<LandOutcomeV1> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionServiceTransportResponseV1 {
    pub schema: String,
    pub request_id: String,
    pub result: MissionServiceTransportResultV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_projection: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionServiceTransportRefusalV1 {
    pub schema: String,
    pub request_id: Option<String>,
    pub code: String,
    pub detail: String,
}

#[derive(Debug)]
pub enum MissionServiceTransportError {
    Decode(serde_json::Error),
    Refused { code: &'static str, detail: String },
    MissionService(MissionServiceError),
    EvidenceProjection(EvidenceSpineError),
}

impl MissionServiceTransportError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Decode(_) => "invalid_transport_request",
            Self::Refused { code, .. } => code,
            Self::MissionService(error) => error.code(),
            Self::EvidenceProjection(error) => error.code(),
        }
    }

    pub fn to_refusal(&self, request_id: Option<&str>) -> MissionServiceTransportRefusalV1 {
        MissionServiceTransportRefusalV1 {
            schema: MISSION_SERVICE_TRANSPORT_REFUSAL_SCHEMA.to_string(),
            request_id: request_id.map(str::to_string),
            code: self.code().to_string(),
            detail: self.to_string(),
        }
    }

    pub fn refused(code: &'static str, detail: impl Into<String>) -> Self {
        Self::Refused {
            code,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for MissionServiceTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => write!(formatter, "invalid external mission request: {error}"),
            Self::Refused { code, detail } => write!(formatter, "{code}: {detail}"),
            Self::MissionService(error) => write!(formatter, "{error}"),
            Self::EvidenceProjection(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for MissionServiceTransportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            Self::MissionService(error) => Some(error),
            Self::EvidenceProjection(error) => Some(error),
            Self::Refused { .. } => None,
        }
    }
}

impl From<serde_json::Error> for MissionServiceTransportError {
    fn from(error: serde_json::Error) -> Self {
        Self::Decode(error)
    }
}

impl From<MissionServiceError> for MissionServiceTransportError {
    fn from(error: MissionServiceError) -> Self {
        Self::MissionService(error)
    }
}

impl From<EvidenceSpineError> for MissionServiceTransportError {
    fn from(error: EvidenceSpineError) -> Self {
        Self::EvidenceProjection(error)
    }
}

impl From<OwnerAuthorizationBrokerError> for MissionServiceTransportError {
    fn from(error: OwnerAuthorizationBrokerError) -> Self {
        Self::refused(error.code(), error.to_string())
    }
}

#[derive(Deserialize)]
struct ExternalActionProbe {
    action: String,
}

/// Decode one strict JSON request. Raw mission-bound mutation names are
/// recognized and refused before authority lookup or payload deserialization,
/// so adding a capability-shaped field can never revive a legacy bypass.
pub fn dispatch_external_mission_json(
    service: &mut MissionService,
    authority: Option<&AuthenticatedAuthorityContextV1>,
    body: &[u8],
    owner_now_ms: u64,
) -> Result<MissionServiceTransportResponseV1, MissionServiceTransportError> {
    let probe: ExternalActionProbe = serde_json::from_slice(body)?;
    if let Some(error) = legacy_mutation_error_with_authority(&probe.action, authority) {
        return Err(error);
    }
    let request: ExternalMissionServiceRequestV1 = serde_json::from_slice(body)?;
    dispatch_external_mission_request(service, authority, &request, owner_now_ms)
}

/// Map one already-decoded external request into the narrow MissionService API.
/// No field is normalized, defaulted, or recomputed at this boundary.
pub fn dispatch_external_mission_request(
    service: &mut MissionService,
    authority: Option<&AuthenticatedAuthorityContextV1>,
    request: &ExternalMissionServiceRequestV1,
    owner_now_ms: u64,
) -> Result<MissionServiceTransportResponseV1, MissionServiceTransportError> {
    dispatch_external_mission_request_with_coordinator(
        service,
        authority,
        request,
        owner_now_ms,
        None,
    )
}

fn dispatch_external_mission_request_with_coordinator(
    service: &mut MissionService,
    authority: Option<&AuthenticatedAuthorityContextV1>,
    request: &ExternalMissionServiceRequestV1,
    owner_now_ms: u64,
    commit_coordinator: Option<&dyn AuthorityWalCommitCoordinator>,
) -> Result<MissionServiceTransportResponseV1, MissionServiceTransportError> {
    validate_metadata(request)?;
    let authority_object_digest = request.authority_object_digest()?;
    let request_id = request.request_id().to_string();
    let result = match request {
        ExternalMissionServiceRequestV1::LandIntent {
            mission_id,
            expected_head_id,
            candidate_id,
            expected_candidate_digest,
            expected_store_version,
            idempotency_key,
            ..
        } => {
            let authority = require_authority(authority)?;
            authority.validate_for(service.brain_id(), &authority_object_digest, owner_now_ms)?;
            MissionServiceTransportResultV1::LandIntent {
                intent: Box::new(service.canonical_land_intent(
                    mission_id,
                    expected_head_id,
                    candidate_id,
                    expected_candidate_digest,
                    *expected_store_version,
                    idempotency_key,
                )?),
            }
        }
        ExternalMissionServiceRequestV1::MissionTransition {
            intent, payload, ..
        } => {
            if intent.to_state == MissionState::Landed {
                return Err(refuse_external_legacy_mutation(
                    LegacyMutationIngress::RawLanded,
                    authority,
                )
                .expect_err("raw landed guard is fail-closed")
                .into());
            }
            if transition_uses_execution_lifecycle(intent, payload) {
                return Err(MissionServiceTransportError::refused(
                    "execution_lifecycle_route_required",
                    "dispatch, ACK, and execution result transitions require their dedicated external action",
                ));
            }
            let authority = require_authority(authority)?;
            MissionServiceTransportResultV1::MissionTransition {
                outcome: Box::new(service.transition(authority, intent, payload, owner_now_ms)?),
            }
        }
        ExternalMissionServiceRequestV1::ExecutionDispatch {
            intent, payload, ..
        } => {
            let authority = require_authority(authority)?;
            MissionServiceTransportResultV1::MissionTransition {
                outcome: Box::new(service.create_execution_dispatch(
                    authority,
                    intent,
                    payload,
                    owner_now_ms,
                )?),
            }
        }
        ExternalMissionServiceRequestV1::ExecutionStarted {
            snapshot,
            intent,
            payload,
            ..
        } => {
            let authority = require_authority(authority)?;
            MissionServiceTransportResultV1::MissionTransition {
                outcome: Box::new(service.reconcile_runner_started_snapshot(
                    authority,
                    snapshot,
                    intent,
                    payload,
                    owner_now_ms,
                )?),
            }
        }
        ExternalMissionServiceRequestV1::ExecutionTerminal {
            snapshot,
            intent,
            payload,
            ..
        } => {
            let authority = require_authority(authority)?;
            MissionServiceTransportResultV1::MissionTransition {
                outcome: Box::new(service.reconcile_runner_terminal_snapshot(
                    authority,
                    snapshot,
                    intent,
                    payload,
                    owner_now_ms,
                )?),
            }
        }
        ExternalMissionServiceRequestV1::Land { request, .. } => {
            let authority = require_authority(authority)?;
            MissionServiceTransportResultV1::Land {
                outcome: Box::new(match commit_coordinator {
                    Some(coordinator) => service.land_with_commit_coordinator(
                        authority,
                        request,
                        owner_now_ms,
                        coordinator,
                    )?,
                    None => service.land(authority, request, owner_now_ms)?,
                }),
            }
        }
    };
    Ok(MissionServiceTransportResponseV1 {
        schema: MISSION_SERVICE_TRANSPORT_RESPONSE_SCHEMA.to_string(),
        request_id,
        result,
        evidence_projection: None,
    })
}

fn validate_metadata(
    request: &ExternalMissionServiceRequestV1,
) -> Result<(), MissionServiceTransportError> {
    if request.schema() != MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA {
        return Err(MissionServiceTransportError::refused(
            "transport_schema_mismatch",
            format!(
                "expected '{}', observed '{}'",
                MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA,
                request.schema()
            ),
        ));
    }
    if request.request_id().trim().is_empty() {
        return Err(MissionServiceTransportError::refused(
            "empty_transport_request_id",
            "request_id must be non-empty",
        ));
    }
    Ok(())
}

fn require_authority(
    authority: Option<&AuthenticatedAuthorityContextV1>,
) -> Result<&AuthenticatedAuthorityContextV1, MissionServiceTransportError> {
    authority.ok_or_else(|| {
        MissionServiceTransportError::refused(
            "missing_authenticated_authority",
            "mutating MissionService requests require owner-injected authenticated authority",
        )
    })
}

pub fn legacy_mutation_refusal(action: &str) -> Option<MissionServiceTransportRefusalV1> {
    legacy_mutation_error(action).map(|error| error.to_refusal(None))
}

fn legacy_mutation_error(action: &str) -> Option<MissionServiceTransportError> {
    legacy_mutation_error_with_authority(action, None)
}

fn legacy_mutation_error_with_authority(
    action: &str,
    authority: Option<&AuthenticatedAuthorityContextV1>,
) -> Option<MissionServiceTransportError> {
    legacy_ingress(action).map(|ingress| {
        refuse_external_legacy_mutation(ingress, authority)
            .expect_err("legacy ingress guard is fail-closed")
            .into()
    })
}

fn legacy_ingress(action: &str) -> Option<LegacyMutationIngress> {
    match action {
        "mission_post" => Some(LegacyMutationIngress::RawMissionPost),
        "receipt_import" => Some(LegacyMutationIngress::ReceiptImport),
        "landed" => Some(LegacyMutationIngress::RawLanded),
        _ => None,
    }
}

fn transition_uses_execution_lifecycle(
    intent: &MissionTransitionIntentV1,
    payload: &MissionTransitionPayloadV1,
) -> bool {
    if intent.to_state == MissionState::Dispatching {
        return true;
    }
    match &payload.evidence {
        MissionTransitionEvidenceV1::MissionServiceDecision { dispatch, .. }
        | MissionTransitionEvidenceV1::AuthorProposal { dispatch, .. }
        | MissionTransitionEvidenceV1::ReviewResult { dispatch, .. } => dispatch.is_some(),
        MissionTransitionEvidenceV1::ExecutionDispatchAck { .. }
        | MissionTransitionEvidenceV1::ExecutionResult { .. } => true,
    }
}

#[cfg(test)]
mod signed_artifact_tests {
    use std::collections::BTreeMap;

    use ed25519_dalek::{Signer as _, SigningKey};
    use m1nd_control::{
        sign_canonical_authority_payload, AuthoritySigner, AuthoritySignerError, ExecutionOutcome,
        ExecutionResultV1, IdentityStatus, MissionTransitionIntentV1, MissionTransitionSource,
        OpaqueSignature, ReviewDecision, ReviewResultV1, Role, VerificationKeyRegistryV1,
        VerificationKeyV1, ED25519_ALGORITHM, EXECUTION_RESULT_SCHEMA,
        MISSION_TRANSITION_INTENT_SCHEMA, REVIEW_RESULT_SCHEMA, VERIFICATION_KEY_REGISTRY_SCHEMA,
    };

    use super::*;

    const NOW: u64 = 100;

    fn hash(label: &str) -> String {
        digest_canonical("signed-transition-evidence-test-v1", &label).unwrap()
    }

    struct FixtureSigner(SigningKey);

    impl FixtureSigner {
        fn new() -> Self {
            Self(SigningKey::from_bytes(&[13u8; 32]))
        }
    }

    impl AuthoritySigner for FixtureSigner {
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
            Ok(self.0.verifying_key().to_bytes().to_vec())
        }

        fn sign(&self, message: &[u8]) -> Result<Vec<u8>, AuthoritySignerError> {
            Ok(self.0.sign(message).to_bytes().to_vec())
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn keys(signer: &FixtureSigner) -> VerificationKeyRegistryV1 {
        VerificationKeyRegistryV1 {
            schema: VERIFICATION_KEY_REGISTRY_SCHEMA.to_string(),
            registry_epoch: 1,
            keys: BTreeMap::from([(
                "owner-key-1".to_string(),
                VerificationKeyV1 {
                    key_id: "owner-key-1".to_string(),
                    subject_id: "owner-1".to_string(),
                    algorithm: ED25519_ALGORITHM.to_string(),
                    public_key: hex(&signer.0.verifying_key().to_bytes()),
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

    fn provider(keys: VerificationKeyRegistryV1) -> OwnerBrokerMissionServiceAuthorityProviderV1 {
        OwnerBrokerMissionServiceAuthorityProviderV1 {
            broker_config: OwnerAuthorizationBrokerConfigV1 {
                root: PathBuf::from("unused-signed-artifact-test-broker"),
                reservation_ttl_ms: 1,
                minimum_terminal_retention_ms: 1,
            },
            linearization: OwnerAuthorityLinearizationV1::default(),
            broker_operation: Arc::new(Mutex::new(())),
            current_authority: Arc::new(|| Err("unused status reader".to_string())),
            protected_journal_head: None,
            transaction_verification_keys: Some(Arc::new(keys)),
            max_future_clock_skew_ms: 10,
            receipt_crypto: None,
        }
    }

    fn intent(source: MissionTransitionSource, source_digest: String) -> MissionTransitionIntentV1 {
        MissionTransitionIntentV1 {
            schema: MISSION_TRANSITION_INTENT_SCHEMA.to_string(),
            transition_id: "transition-1".to_string(),
            brain_id: "brain-1".to_string(),
            mission_id: "mission-1".to_string(),
            expected_head_id: Some("head-1".to_string()),
            from_state: Some(MissionState::Executing),
            to_state: MissionState::Gate,
            iteration_id: 1,
            actor_id: "owner-1".to_string(),
            role: Role::Runner,
            source,
            source_digest,
            capability_id: "capability-1".to_string(),
            packet_digest: hash("packet"),
            payload_digest: hash("payload"),
            idempotency_key: "transition-idempotency-1".to_string(), // gitleaks:allow
            causation_id: None,
            issued_at: 90,
            expires_at: 200,
            issuer: "owner-1".to_string(),
            key_id: "owner-key-1".to_string(),
            algorithm: ED25519_ALGORITHM.to_string(),
            intent_digest: hash("intent"),
            signature: OpaqueSignature::new("intent-signature-not-under-test"),
        }
    }

    fn request(evidence: MissionTransitionEvidenceV1) -> ExternalMissionServiceRequestV1 {
        let (source, source_digest) = match &evidence {
            MissionTransitionEvidenceV1::ExecutionResult { result, .. } => (
                MissionTransitionSource::ExecutionResult,
                result.result_digest.clone(),
            ),
            MissionTransitionEvidenceV1::ReviewResult { result, .. } => (
                MissionTransitionSource::ReviewResult,
                result.result_digest.clone(),
            ),
            _ => unreachable!(),
        };
        ExternalMissionServiceRequestV1::MissionTransition {
            schema: MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA.to_string(),
            request_id: "signed-evidence-request".to_string(),
            intent: intent(source, source_digest),
            payload: MissionTransitionPayloadV1 {
                schema: crate::mission_service::MISSION_TRANSITION_PAYLOAD_SCHEMA.to_string(),
                brain_id: "brain-1".to_string(),
                mission_id: "mission-1".to_string(),
                block_id: "block-1".to_string(),
                expected_store_version: 1,
                expected_boundary_version: 1,
                expected_contract_version: 1,
                evidence,
            },
        }
    }

    fn sign_execution(signer: &FixtureSigner, key: &VerificationKeyV1) -> ExecutionResultV1 {
        let mut result = ExecutionResultV1 {
            schema: EXECUTION_RESULT_SCHEMA.to_string(),
            result_id: "execution-result-1".to_string(),
            execution_id: "execution-1".to_string(),
            dispatch_digest: hash("dispatch"),
            brain_id: "brain-1".to_string(),
            mission_id: "mission-1".to_string(),
            mission_head_id: "head-1".to_string(),
            iteration_id: 1,
            runner_id: "owner-1".to_string(),
            outcome: ExecutionOutcome::Succeeded,
            command: vec!["cargo".to_string(), "test".to_string()],
            exit_status: Some(0),
            started_at: 80,
            ended_at: 90,
            log_digest: hash("execution-log"),
            failure_artifact_digest: None,
            issuer: "owner-1".to_string(),
            key_id: "owner-key-1".to_string(),
            algorithm: ED25519_ALGORITHM.to_string(),
            result_digest: String::new(),
            signature: OpaqueSignature::new("pending"),
        };
        result.seal().unwrap();
        let payload = result.canonical_signature_payload().unwrap();
        result.signature = sign_canonical_authority_payload(
            m1nd_control::EXECUTION_RESULT_SIGNATURE_DOMAIN,
            &payload,
            key,
            signer,
        )
        .unwrap();
        result
    }

    fn sign_review(signer: &FixtureSigner, key: &VerificationKeyV1) -> ReviewResultV1 {
        let mut result = ReviewResultV1 {
            schema: REVIEW_RESULT_SCHEMA.to_string(),
            result_id: "review-result-1".to_string(),
            brain_id: "brain-1".to_string(),
            mission_id: "mission-1".to_string(),
            mission_head_id: "head-1".to_string(),
            iteration_id: 1,
            reviewer_id: "owner-1".to_string(),
            reviewed_state: MissionState::Review,
            packet_digest: hash("packet"),
            decision: ReviewDecision::Approve,
            verdict_digest: hash("verdict"),
            binding_changes_digest: None,
            gate_digest: Some(hash("gate")),
            candidate_digest: Some(hash("candidate")),
            issued_at: 90,
            issuer: "owner-1".to_string(),
            key_id: "owner-key-1".to_string(),
            algorithm: ED25519_ALGORITHM.to_string(),
            result_digest: String::new(),
            signature: OpaqueSignature::new("pending"),
        };
        result.seal().unwrap();
        let payload = result.canonical_signature_payload().unwrap();
        result.signature = sign_canonical_authority_payload(
            m1nd_control::REVIEW_RESULT_SIGNATURE_DOMAIN,
            &payload,
            key,
            signer,
        )
        .unwrap();
        result
    }

    #[test]
    fn execution_and_review_results_require_owner_pinned_non_circular_signatures() {
        let signer = FixtureSigner::new();
        let keys = keys(&signer);
        let provider = provider(keys.clone());
        let key = keys.keys.get("owner-key-1").unwrap();

        let execution = sign_execution(&signer, key);
        provider
            .verify_signed_transition_evidence(
                &request(MissionTransitionEvidenceV1::ExecutionResult {
                    result: execution.clone(),
                    candidate: None,
                }),
                NOW,
            )
            .unwrap();
        let mut execution_tamper = execution;
        execution_tamper.log_digest = hash("tampered-execution-log");
        assert_eq!(
            provider
                .verify_signed_transition_evidence(
                    &request(MissionTransitionEvidenceV1::ExecutionResult {
                        result: execution_tamper,
                        candidate: None,
                    }),
                    NOW,
                )
                .unwrap_err()
                .code(),
            "signed_artifact_signature_invalid"
        );

        let review = sign_review(&signer, key);
        provider
            .verify_signed_transition_evidence(
                &request(MissionTransitionEvidenceV1::ReviewResult {
                    result: review.clone(),
                    dispatch: None,
                }),
                NOW,
            )
            .unwrap();
        let mut review_tamper = review;
        review_tamper.verdict_digest = hash("tampered-verdict");
        assert_eq!(
            provider
                .verify_signed_transition_evidence(
                    &request(MissionTransitionEvidenceV1::ReviewResult {
                        result: review_tamper,
                        dispatch: None,
                    }),
                    NOW,
                )
                .unwrap_err()
                .code(),
            "signed_artifact_signature_invalid"
        );
    }
}
