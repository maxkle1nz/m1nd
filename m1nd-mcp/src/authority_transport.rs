//! Strict owner ingress for minting one-shot G2 authorization leases.
//!
//! The request body can carry signed authority artifacts, but never a key
//! registry, owner time, wire session, ingress context, or brain selection.
//! Those are injected from owner configuration and the authenticated transport.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

use m1nd_control::{
    digest_canonical, ActionId, AuthorityCapabilityV1, CapabilityKind, Effect, Ingress, Role,
    VerificationKeyRegistryV1,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::authority_runtime::{
    AuthenticatedSessionV1, AuthorityAuthorizationReceiptV1, AuthorityAuthorizationRequestV1,
    AuthorityInputV1, AuthorityRuntime, AuthorityRuntimeError, AuthorityVerificationAssurance,
    PositiveSovereignAuthorityMetadataV1, SafetyActuatorAttemptV1, ServiceIdentityAssertionV1,
    SessionChallengeRequestV1, SessionChallengeV1, AUTHORIZATION_RECEIPT_SCHEMA,
    AUTHORIZATION_RECEIPT_SIGNATURE_DOMAIN,
};
use crate::authority_wal::{AuthorityWalCryptoAssurance, AuthorityWalRecordCrypto};
use crate::autonomy_manifest::AutonomyAuthorityEvidenceV1;
use crate::mission_service_transport::{
    AuthorityStatusReader, MissionServiceIngressV1, MissionServiceTransportContextV1,
    OwnerBrokerAuthorityProviderInputsV1, OwnerBrokerMissionServiceAuthorityProviderV1,
};
use crate::owner_authorization_broker::{
    OwnerAuthorityLinearizationV1, OwnerAuthorizationBrokerConfigV1, OwnerAuthorizationBrokerError,
    OwnerAuthorizationBrokerV1,
};
use crate::protected_journal_head::SharedProtectedJournalHeadBackendV1;

pub const AUTHORITY_AUTHORIZE_REQUEST_SCHEMA: &str = "m1nd-authority-authorize-request-v1";
pub const AUTHORITY_AUTHORIZE_RESPONSE_SCHEMA: &str = "m1nd-authority-authorize-response-v1";
pub const AUTHORITY_TRANSPORT_REFUSAL_SCHEMA: &str = "m1nd-authority-transport-refusal-v1";
pub const AUTHORIZATION_LEASE_ID_DIGEST_DOMAIN: &str = "m1nd-owner-authorization-lease-id-v1";
pub const AUTHORITY_SESSION_CHALLENGE_REQUEST_SCHEMA: &str =
    "m1nd-authority-session-challenge-request-v1";
pub const AUTHORITY_SESSION_CHALLENGE_RESPONSE_SCHEMA: &str =
    "m1nd-authority-session-challenge-response-v1";
pub const AUTHORITY_SESSION_AUTHENTICATE_REQUEST_SCHEMA: &str =
    "m1nd-authority-session-authenticate-request-v1";
pub const AUTHORITY_SESSION_AUTHENTICATE_RESPONSE_SCHEMA: &str =
    "m1nd-authority-session-authenticate-response-v1";
pub const AUTHORITY_SESSION_CHALLENGE_ID_DIGEST_DOMAIN: &str =
    "m1nd-authority-session-challenge-id-v1";
pub const MAX_AUTHORITY_SESSION_CHALLENGE_TTL_MS: u64 = 5 * 60 * 1_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoritySessionChallengeRequestV1 {
    pub schema: String,
    pub request_id: String,
    pub subject_id: String,
    pub key_id: String,
    pub app_host_identity: String,
    pub nonce: String,
    pub requested_ttl_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoritySessionChallengeResponseV1 {
    pub schema: String,
    pub request_id: String,
    pub challenge: SessionChallengeV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoritySessionAuthenticateRequestV1 {
    pub schema: String,
    pub request_id: String,
    pub challenge_id: String,
    pub capability: AuthorityCapabilityV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthoritySessionVerificationAssuranceV1 {
    ControlVerifiedEd25519,
    ControlVerifiedEcdsaP256Sha256X962,
    SoftwareTestOnlyNotProven,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticatedAuthoritySessionWireV1 {
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
    pub verification_assurance: AuthoritySessionVerificationAssuranceV1,
}

impl From<AuthenticatedSessionV1> for AuthenticatedAuthoritySessionWireV1 {
    fn from(session: AuthenticatedSessionV1) -> Self {
        Self {
            session_id: session.session_id,
            subject_id: session.subject_id,
            key_id: session.key_id,
            app_host_identity: session.app_host_identity,
            audience: session.audience,
            session_context_digest: session.session_context_digest,
            key_registry_epoch: session.key_registry_epoch,
            authenticated_at: session.authenticated_at,
            expires_at: session.expires_at,
            authentication_body_digest: session.authentication_body_digest,
            verification_assurance: match session.verification_assurance {
                AuthorityVerificationAssurance::ControlVerifiedEd25519 => {
                    AuthoritySessionVerificationAssuranceV1::ControlVerifiedEd25519
                }
                AuthorityVerificationAssurance::ControlVerifiedEcdsaP256Sha256X962 => {
                    AuthoritySessionVerificationAssuranceV1::ControlVerifiedEcdsaP256Sha256X962
                }
                AuthorityVerificationAssurance::SoftwareTestOnlyNotProven => {
                    AuthoritySessionVerificationAssuranceV1::SoftwareTestOnlyNotProven
                }
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoritySessionAuthenticateResponseV1 {
    pub schema: String,
    pub request_id: String,
    pub session: AuthenticatedAuthoritySessionWireV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "authority", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthorityAuthorizeInputV1 {
    OrdinarySession {
        role: Role,
    },
    PositiveSovereign {
        capability: Box<AuthorityCapabilityV1>,
        role: Role,
        capability_kind: CapabilityKind,
        authority_decision_digest: String,
        applicable_grant_id: Option<String>,
        applicable_tier: Option<m1nd_control::AutonomyTier>,
        autonomy_evidence: Option<Box<AutonomyAuthorityEvidenceV1>>,
    },
    ServiceIdentity {
        assertion: ServiceIdentityAssertionV1,
    },
    Safety {
        attempt: SafetyActuatorAttemptV1,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityAuthorizeRequestV1 {
    pub schema: String,
    pub request_id: String,
    /// G2 authenticated session; never the REST bearer or MCP wire session.
    pub authority_session_id: Option<String>,
    pub authority_session_context_digest: Option<String>,
    pub target_action: String,
    pub payload_digest: String,
    pub requested_effects: BTreeSet<Effect>,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
    pub input: AuthorityAuthorizeInputV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityAuthorizeResponseV1 {
    pub schema: String,
    pub request_id: String,
    pub authorization_lease_id: String,
    pub authorization_receipt: AuthorityAuthorizationReceiptV1,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityTransportRefusalV1 {
    pub schema: String,
    pub request_id: Option<String>,
    pub code: String,
    pub detail: String,
}

#[derive(Debug)]
pub enum AuthorityTransportError {
    Refused { code: &'static str, detail: String },
    Canonical(m1nd_control::CanonicalError),
    Runtime(AuthorityRuntimeError),
    Broker(OwnerAuthorizationBrokerError),
}

impl AuthorityTransportError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Refused { code, .. } => code,
            Self::Runtime(error) => error.code(),
            Self::Broker(error) => error.code(),
            Self::Canonical(_) => "authority_transport_canonicalization_failed",
        }
    }

    pub(crate) fn refused(code: &'static str, detail: impl Into<String>) -> Self {
        Self::Refused {
            code,
            detail: detail.into(),
        }
    }

    pub fn to_refusal(&self, request_id: Option<&str>) -> AuthorityTransportRefusalV1 {
        AuthorityTransportRefusalV1 {
            schema: AUTHORITY_TRANSPORT_REFUSAL_SCHEMA.to_string(),
            request_id: request_id.map(str::to_string),
            code: self.code().to_string(),
            detail: self.to_string(),
        }
    }
}

impl fmt::Display for AuthorityTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused { code, detail } => write!(formatter, "{code}: {detail}"),
            Self::Runtime(error) => write!(formatter, "authority runtime: {error}"),
            Self::Broker(error) => write!(formatter, "authorization broker: {error}"),
            Self::Canonical(error) => write!(formatter, "authority canonicalization: {error}"),
        }
    }
}

impl Error for AuthorityTransportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Runtime(error) => Some(error),
            Self::Broker(error) => Some(error),
            Self::Canonical(error) => Some(error),
            Self::Refused { .. } => None,
        }
    }
}

impl From<AuthorityRuntimeError> for AuthorityTransportError {
    fn from(error: AuthorityRuntimeError) -> Self {
        Self::Runtime(error)
    }
}

impl From<OwnerAuthorizationBrokerError> for AuthorityTransportError {
    fn from(error: OwnerAuthorizationBrokerError) -> Self {
        Self::Broker(error)
    }
}

impl From<m1nd_control::CanonicalError> for AuthorityTransportError {
    fn from(error: m1nd_control::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

/// Shared production coordinator for G2 authorization and G3 lease consumption.
/// The operation mutex is the process-local realization of
/// `OWNER_AUTHORITY_TRANSACTION_V1`; durable recovery still uses the broker
/// journal and AuthorityWAL witnesses rather than trusting process memory.
pub struct OwnerAuthorityServiceV1 {
    runtime: Arc<AuthorityRuntime>,
    verification_keys: Arc<VerificationKeyRegistryV1>,
    session_roles: Arc<BTreeMap<String, Role>>,
    broker_config: OwnerAuthorizationBrokerConfigV1,
    linearization: OwnerAuthorityLinearizationV1,
    broker_operation: Arc<Mutex<()>>,
    protected_journal_head: SharedProtectedJournalHeadBackendV1,
    receipt_crypto: Arc<dyn AuthorityWalRecordCrypto>,
}

impl OwnerAuthorityServiceV1 {
    pub fn issue_session_challenge(
        &self,
        context: &MissionServiceTransportContextV1,
        request: AuthoritySessionChallengeRequestV1,
        owner_now_ms: u64,
    ) -> Result<AuthoritySessionChallengeResponseV1, AuthorityTransportError> {
        if request.schema != AUTHORITY_SESSION_CHALLENGE_REQUEST_SCHEMA
            || request.request_id.trim().is_empty()
            || request.subject_id.trim().is_empty()
            || request.key_id.trim().is_empty()
            || request.app_host_identity.trim().is_empty()
            || request.nonce.trim().is_empty()
            || request.requested_ttl_ms == 0
            || request.requested_ttl_ms > MAX_AUTHORITY_SESSION_CHALLENGE_TTL_MS
        {
            return Err(AuthorityTransportError::refused(
                "invalid_authority_session_challenge_request",
                "strict schema, identity, nonce, and bounded non-zero TTL are required",
            ));
        }
        let transport_session_id = required_context(
            context.transport_session_id.as_deref(),
            "missing_transport_session",
        )?;
        let session_context_digest = required_context(
            context.ingress_context_digest.as_deref(),
            "missing_ingress_context_digest",
        )?;
        let brain_id =
            required_context(context.actor_brain_id.as_deref(), "missing_actor_brain_id")?;
        if self.runtime.status()?.state.core.brain_id != brain_id {
            return Err(AuthorityTransportError::refused(
                "authority_brain_mismatch",
                "transport brain differs from the owner authority runtime",
            ));
        }
        let challenge_id = digest_canonical(
            AUTHORITY_SESSION_CHALLENGE_ID_DIGEST_DOMAIN,
            &(
                request.request_id.as_str(),
                request.subject_id.as_str(),
                request.key_id.as_str(),
                request.app_host_identity.as_str(),
                request.nonce.as_str(),
                transport_session_id,
                session_context_digest,
                owner_now_ms,
            ),
        )?;
        let _operation = self.broker_operation.lock();
        let challenge = self.runtime.issue_session_challenge(
            SessionChallengeRequestV1 {
                challenge_id,
                subject_id: request.subject_id,
                key_id: request.key_id,
                app_host_identity: request.app_host_identity,
                session_context_digest: session_context_digest.to_string(),
                nonce: request.nonce,
                issued_at: owner_now_ms,
                expires_at: owner_now_ms.saturating_add(request.requested_ttl_ms),
            },
            &self.verification_keys,
            owner_now_ms,
        )?;
        Ok(AuthoritySessionChallengeResponseV1 {
            schema: AUTHORITY_SESSION_CHALLENGE_RESPONSE_SCHEMA.to_string(),
            request_id: request.request_id,
            challenge,
        })
    }

    pub fn authenticate_session(
        &self,
        context: &MissionServiceTransportContextV1,
        request: AuthoritySessionAuthenticateRequestV1,
        owner_now_ms: u64,
    ) -> Result<AuthoritySessionAuthenticateResponseV1, AuthorityTransportError> {
        if request.schema != AUTHORITY_SESSION_AUTHENTICATE_REQUEST_SCHEMA
            || request.request_id.trim().is_empty()
            || request.challenge_id.trim().is_empty()
        {
            return Err(AuthorityTransportError::refused(
                "invalid_authority_session_authenticate_request",
                "strict schema, request id, challenge id, and signed capability are required",
            ));
        }
        let session_context_digest = required_context(
            context.ingress_context_digest.as_deref(),
            "missing_ingress_context_digest",
        )?;
        let brain_id =
            required_context(context.actor_brain_id.as_deref(), "missing_actor_brain_id")?;
        if self.runtime.status()?.state.core.brain_id != brain_id {
            return Err(AuthorityTransportError::refused(
                "authority_brain_mismatch",
                "transport brain differs from the owner authority runtime",
            ));
        }
        let challenge = self
            .runtime
            .pending_session_challenge(&request.challenge_id, owner_now_ms)?
            .ok_or_else(|| {
                AuthorityTransportError::refused(
                    "authority_session_challenge_not_pending",
                    "challenge does not exist or was already consumed",
                )
            })?;
        if challenge.core.session_context_digest != session_context_digest {
            return Err(AuthorityTransportError::refused(
                "authority_session_transport_mismatch",
                "challenge is bound to a different owner-observed correlation context",
            ));
        }
        let ingress = match context.ingress {
            MissionServiceIngressV1::Rest => Ingress::Rest,
            MissionServiceIngressV1::McpStreamableHttp => Ingress::Mcp,
        };
        let _operation = self.broker_operation.lock();
        let session = self.runtime.authenticate_session(
            &request.challenge_id,
            ingress,
            &request.capability,
            &self.verification_keys,
            owner_now_ms,
        )?;
        Ok(AuthoritySessionAuthenticateResponseV1 {
            schema: AUTHORITY_SESSION_AUTHENTICATE_RESPONSE_SCHEMA.to_string(),
            request_id: request.request_id,
            session: session.into(),
        })
    }

    pub fn authorize(
        &self,
        context: &MissionServiceTransportContextV1,
        request: AuthorityAuthorizeRequestV1,
        owner_now_ms: u64,
    ) -> Result<AuthorityAuthorizeResponseV1, AuthorityTransportError> {
        if request.schema != AUTHORITY_AUTHORIZE_REQUEST_SCHEMA
            || request.request_id.trim().is_empty()
            || request.target_action.trim().is_empty()
            || request.requested_effects.is_empty()
        {
            return Err(AuthorityTransportError::refused(
                "invalid_authority_authorize_request",
                "schema, request id, target action, and complete effects are required",
            ));
        }
        let transport_session_id = required_context(
            context.transport_session_id.as_deref(),
            "missing_transport_session",
        )?;
        let ingress_context_digest = required_context(
            context.ingress_context_digest.as_deref(),
            "missing_ingress_context_digest",
        )?;
        let brain_id =
            required_context(context.actor_brain_id.as_deref(), "missing_actor_brain_id")?;
        let status = self.runtime.status()?;
        if status.state.core.brain_id != brain_id {
            return Err(AuthorityTransportError::refused(
                "authority_brain_mismatch",
                "transport brain differs from the owner authority runtime",
            ));
        }
        let ingress = match context.ingress {
            MissionServiceIngressV1::Rest => Ingress::Rest,
            MissionServiceIngressV1::McpStreamableHttp => Ingress::Mcp,
        };
        let pinned_session_role = match &request.input {
            AuthorityAuthorizeInputV1::OrdinarySession { .. }
            | AuthorityAuthorizeInputV1::PositiveSovereign { .. } => {
                let session_id = request.authority_session_id.as_deref().ok_or_else(|| {
                    AuthorityTransportError::refused(
                        "missing_authority_session",
                        "ordinary and positive authority require an authenticated G2 session",
                    )
                })?;
                if request.authority_session_context_digest.as_deref()
                    != Some(ingress_context_digest)
                {
                    return Err(AuthorityTransportError::refused(
                        "authority_session_transport_mismatch",
                        "authenticated G2 session is bound to a different owner-observed correlation context",
                    ));
                }
                let session = self
                    .runtime
                    .authenticated_session(session_id, owner_now_ms)?
                    .ok_or_else(|| {
                        AuthorityTransportError::refused(
                            "authority_session_not_found",
                            "authenticated G2 session does not exist",
                        )
                    })?;
                if session.session_context_digest != ingress_context_digest {
                    return Err(AuthorityTransportError::refused(
                        "authority_session_transport_mismatch",
                        "stored G2 session is bound to a different owner-observed correlation context",
                    ));
                }
                let pinned_role = self
                    .session_roles
                    .get(&session.subject_id)
                    .copied()
                    .ok_or_else(|| {
                        AuthorityTransportError::refused(
                            "authority_session_role_not_pinned",
                            "authenticated subject has no owner-pinned operational role",
                        )
                    })?;
                if pinned_role == Role::MissionService {
                    return Err(AuthorityTransportError::refused(
                        "authority_session_role_invalid",
                        "owner sessions cannot assume the MissionService service role",
                    ));
                }
                Some(pinned_role)
            }
            AuthorityAuthorizeInputV1::ServiceIdentity { .. }
            | AuthorityAuthorizeInputV1::Safety { .. }
                if request.authority_session_id.is_some()
                    || request.authority_session_context_digest.is_some() =>
            {
                return Err(AuthorityTransportError::refused(
                    "unexpected_authority_session",
                    "service and safety authority paths are disjoint from owner sessions",
                ));
            }
            _ => None,
        };
        let runtime_request = AuthorityAuthorizationRequestV1 {
            session_id: request.authority_session_id.clone(),
            session_context_digest: request.authority_session_context_digest.clone(),
            transport_session_id: transport_session_id.to_string(),
            ingress_context_digest: ingress_context_digest.to_string(),
            ingress,
            action: ActionId::new(&request.target_action).map_err(|error| {
                AuthorityTransportError::refused("invalid_target_action", error.to_string())
            })?,
            payload_digest: request.payload_digest.clone(),
            requested_effects: request.requested_effects.clone(),
            mission_id: request.mission_id.clone(),
            mission_head_id: request.mission_head_id.clone(),
            now_ms: owner_now_ms,
        };

        let _operation = self.broker_operation.lock();
        let mut receipt = match &request.input {
            AuthorityAuthorizeInputV1::OrdinarySession { role } => {
                let pinned_role = pinned_session_role.expect("ordinary session validated above");
                if *role != pinned_role {
                    return Err(AuthorityTransportError::refused(
                        "authority_session_role_mismatch",
                        "wire role differs from the owner-pinned subject role",
                    ));
                }
                self.runtime.authorize_mutation(
                    runtime_request,
                    AuthorityInputV1::OrdinarySession {
                        keys: &self.verification_keys,
                        role: pinned_role,
                    },
                )?
            }
            AuthorityAuthorizeInputV1::PositiveSovereign {
                capability,
                role,
                capability_kind,
                authority_decision_digest,
                applicable_grant_id,
                applicable_tier,
                autonomy_evidence,
            } => {
                let pinned_role = pinned_session_role.expect("positive session validated above");
                if *role != pinned_role {
                    return Err(AuthorityTransportError::refused(
                        "authority_session_role_mismatch",
                        "wire role differs from the owner-pinned subject role",
                    ));
                }
                let metadata = PositiveSovereignAuthorityMetadataV1 {
                    role: pinned_role,
                    capability_kind: *capability_kind,
                    authority_decision_digest: authority_decision_digest.clone(),
                    applicable_grant_id: applicable_grant_id.clone(),
                    applicable_tier: *applicable_tier,
                };
                self.runtime.authorize_mutation(
                    runtime_request,
                    AuthorityInputV1::PositiveSovereign {
                        capability,
                        keys: &self.verification_keys,
                        metadata: &metadata,
                        autonomy_evidence: autonomy_evidence.as_deref(),
                    },
                )?
            }
            AuthorityAuthorizeInputV1::ServiceIdentity { assertion } => {
                self.runtime.authorize_mutation(
                    runtime_request,
                    AuthorityInputV1::ServiceIdentity { assertion },
                )?
            }
            AuthorityAuthorizeInputV1::Safety { attempt } => self
                .runtime
                .authorize_mutation(runtime_request, AuthorityInputV1::Safety { attempt })?,
        };
        sign_authorization_receipt(&mut receipt, self.receipt_crypto.as_ref())?;
        let lease_id = digest_canonical(
            AUTHORIZATION_LEASE_ID_DIGEST_DOMAIN,
            &(
                receipt.receipt_digest.as_str(),
                request.request_id.as_str(),
                transport_session_id,
                ingress_context_digest,
            ),
        )?;
        let mut broker = OwnerAuthorizationBrokerV1::open_with_protected_head(
            self.broker_config.clone(),
            self.linearization.clone(),
            Arc::clone(&self.protected_journal_head),
        )?;
        let lease = broker.issue(&lease_id, receipt.clone(), owner_now_ms)?;
        Ok(AuthorityAuthorizeResponseV1 {
            schema: AUTHORITY_AUTHORIZE_RESPONSE_SCHEMA.to_string(),
            request_id: request.request_id,
            authorization_lease_id: lease_id,
            authorization_receipt: receipt,
            expires_at: lease.expires_at,
        })
    }
}

fn required_context<'a>(
    value: Option<&'a str>,
    code: &'static str,
) -> Result<&'a str, AuthorityTransportError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AuthorityTransportError::refused(
                code,
                "required owner-observed transport context is absent",
            )
        })
}

pub(crate) fn authorization_receipt_signature_message(canonical_payload: &[u8]) -> Vec<u8> {
    const PREFIX: &[u8] = b"m1nd-runtime-authorization-receipt-signature-message-v1\0";
    let domain = AUTHORIZATION_RECEIPT_SIGNATURE_DOMAIN.as_bytes();
    let mut message =
        Vec::with_capacity(PREFIX.len() + domain.len() + canonical_payload.len() + 16);
    message.extend_from_slice(PREFIX);
    message.extend_from_slice(&(domain.len() as u64).to_be_bytes());
    message.extend_from_slice(domain);
    message.extend_from_slice(&(canonical_payload.len() as u64).to_be_bytes());
    message.extend_from_slice(canonical_payload);
    message
}

fn sign_authorization_receipt(
    receipt: &mut AuthorityAuthorizationReceiptV1,
    crypto: &dyn AuthorityWalRecordCrypto,
) -> Result<(), AuthorityTransportError> {
    if crypto.assurance() == AuthorityWalCryptoAssurance::UnavailableFailClosed {
        return Err(AuthorityTransportError::refused(
            "authorization_receipt_signer_not_installed",
            "production authorization receipt signer is NOT_INSTALLED",
        ));
    }
    receipt.schema = AUTHORIZATION_RECEIPT_SCHEMA.to_string();
    receipt.issuer = crypto.issuer().to_string();
    receipt.key_id = crypto.key_id().to_string();
    receipt.algorithm = crypto.algorithm().to_string();
    receipt.signature = m1nd_control::OpaqueSignature::new("pending-owner-signature");
    let canonical = receipt.canonical_signature_payload()?;
    let message = authorization_receipt_signature_message(&canonical);
    receipt.signature =
        m1nd_control::OpaqueSignature::new(crypto.sign(&message).map_err(|detail| {
            AuthorityTransportError::refused("authorization_receipt_signing_failed", detail)
        })?);
    verify_authorization_receipt(receipt, crypto)
}

pub(crate) fn verify_authorization_receipt(
    receipt: &AuthorityAuthorizationReceiptV1,
    crypto: &dyn AuthorityWalRecordCrypto,
) -> Result<(), AuthorityTransportError> {
    if crypto.assurance() == AuthorityWalCryptoAssurance::UnavailableFailClosed {
        return Err(AuthorityTransportError::refused(
            "authorization_receipt_verifier_not_installed",
            "production authorization receipt verifier is NOT_INSTALLED",
        ));
    }
    if receipt.schema != AUTHORIZATION_RECEIPT_SCHEMA
        || receipt.issuer != crypto.issuer()
        || receipt.key_id != crypto.key_id()
        || receipt.algorithm != crypto.algorithm()
        || digest_canonical(
            crate::authority_runtime::AUTHORIZATION_RECEIPT_DIGEST_DOMAIN,
            &receipt.core,
        )? != receipt.receipt_digest
    {
        return Err(AuthorityTransportError::refused(
            "authorization_receipt_binding_mismatch",
            "receipt schema, core digest, or pinned signer metadata differs",
        ));
    }
    let canonical = receipt.canonical_signature_payload()?;
    let message = authorization_receipt_signature_message(&canonical);
    crypto
        .verify(&message, receipt.signature.as_str())
        .map_err(|detail| {
            AuthorityTransportError::refused("authorization_receipt_signature_invalid", detail)
        })
}

pub(crate) struct OwnerAuthorityComponentInputsV1 {
    pub runtime: Arc<AuthorityRuntime>,
    pub verification_keys: Arc<VerificationKeyRegistryV1>,
    pub session_roles: Arc<BTreeMap<String, Role>>,
    pub max_future_clock_skew_ms: u64,
    pub receipt_crypto: Arc<dyn AuthorityWalRecordCrypto>,
    pub broker_config: OwnerAuthorizationBrokerConfigV1,
    pub linearization: OwnerAuthorityLinearizationV1,
    pub protected_journal_head: SharedProtectedJournalHeadBackendV1,
}

/// Assemble the two public production seams from one immutable, fully explicit
/// owner input snapshot. No defaults or environment lookups exist here.
pub(crate) fn owner_authority_components(
    inputs: OwnerAuthorityComponentInputsV1,
) -> (
    Arc<OwnerAuthorityServiceV1>,
    Arc<OwnerBrokerMissionServiceAuthorityProviderV1>,
) {
    let OwnerAuthorityComponentInputsV1 {
        runtime,
        verification_keys,
        session_roles,
        max_future_clock_skew_ms,
        receipt_crypto,
        broker_config,
        linearization,
        protected_journal_head,
    } = inputs;
    let broker_operation = Arc::new(Mutex::new(()));
    let transaction_verification_keys = Arc::clone(&verification_keys);
    let provider_receipt_crypto = Arc::clone(&receipt_crypto);
    let status_runtime = Arc::clone(&runtime);
    let current_authority: Arc<AuthorityStatusReader> =
        Arc::new(move || status_runtime.status().map_err(|error| error.to_string()));
    let service = Arc::new(OwnerAuthorityServiceV1 {
        runtime,
        verification_keys,
        session_roles,
        broker_config: broker_config.clone(),
        linearization: linearization.clone(),
        broker_operation: Arc::clone(&broker_operation),
        protected_journal_head: Arc::clone(&protected_journal_head),
        receipt_crypto,
    });
    let provider = Arc::new(
        OwnerBrokerMissionServiceAuthorityProviderV1::from_owner_inputs(
            OwnerBrokerAuthorityProviderInputsV1 {
                broker_config,
                linearization,
                broker_operation,
                current_authority,
                protected_journal_head,
                transaction_verification_keys,
                max_future_clock_skew_ms,
                receipt_crypto: provider_receipt_crypto,
            },
        ),
    );
    (service, provider)
}

/// Production assembly variant that adds the closed non-mission mutation
/// consumer while preserving the exact same broker coordinator, current-status
/// reader, receipt verifier, linearization point, and protected journal head as
/// issuance and MissionService.
pub(crate) fn owner_authority_components_with_external_mutation(
    inputs: OwnerAuthorityComponentInputsV1,
    journal_root: std::path::PathBuf,
    owner_clock: Arc<dyn Fn() -> u64 + Send + Sync>,
) -> (
    Arc<OwnerAuthorityServiceV1>,
    Arc<OwnerBrokerMissionServiceAuthorityProviderV1>,
    Arc<crate::external_mutation_service::ExternalMutationServiceV1>,
) {
    let OwnerAuthorityComponentInputsV1 {
        runtime,
        verification_keys,
        session_roles,
        max_future_clock_skew_ms,
        receipt_crypto,
        broker_config,
        linearization,
        protected_journal_head,
    } = inputs;
    let broker_operation = Arc::new(Mutex::new(()));
    let transaction_verification_keys = Arc::clone(&verification_keys);
    let provider_receipt_crypto = Arc::clone(&receipt_crypto);
    let external_receipt_crypto = Arc::clone(&receipt_crypto);
    let status_runtime = Arc::clone(&runtime);
    let current_authority: Arc<AuthorityStatusReader> =
        Arc::new(move || status_runtime.status().map_err(|error| error.to_string()));
    let service = Arc::new(OwnerAuthorityServiceV1 {
        runtime,
        verification_keys,
        session_roles,
        broker_config: broker_config.clone(),
        linearization: linearization.clone(),
        broker_operation: Arc::clone(&broker_operation),
        protected_journal_head: Arc::clone(&protected_journal_head),
        receipt_crypto,
    });
    let provider = Arc::new(
        OwnerBrokerMissionServiceAuthorityProviderV1::from_owner_inputs(
            OwnerBrokerAuthorityProviderInputsV1 {
                broker_config: broker_config.clone(),
                linearization: linearization.clone(),
                broker_operation: Arc::clone(&broker_operation),
                current_authority: Arc::clone(&current_authority),
                protected_journal_head: Arc::clone(&protected_journal_head),
                transaction_verification_keys,
                max_future_clock_skew_ms,
                receipt_crypto: provider_receipt_crypto,
            },
        ),
    );
    let external = Arc::new(
        crate::external_mutation_service::ExternalMutationServiceV1::from_owner_inputs(
            crate::external_mutation_service::ExternalMutationServiceInputsV1 {
                journal_root,
                broker_config,
                linearization,
                broker_operation,
                current_authority,
                protected_journal_head,
                receipt_crypto: external_receipt_crypto,
                owner_clock,
            },
        ),
    );
    (service, provider, external)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority_runtime::{AuthorityAuthorizationReceiptCoreV1, AuthorizationAuthorityV1};
    use crate::authority_wal::SoftwareTestAuthorityWalRecordCrypto;
    use m1nd_control::{
        ActiveMode, AuthorityVariant, CapabilityKind, Ingress, OpaqueSignature,
        ReachablePolicyTupleV1, RiskClass,
    };

    fn hash(label: &str) -> String {
        digest_canonical("authority-transport-receipt-test-v1", &label).unwrap()
    }

    #[test]
    fn p256_session_assurance_mirrors_to_wire_without_relabeling() {
        let session = crate::authority_runtime::AuthenticatedSessionV1 {
            session_id: "session-1".to_string(),
            subject_id: "owner-1".to_string(),
            key_id: "owner-p256-key-1".to_string(),
            app_host_identity: "host-1".to_string(),
            audience: "audience-1".to_string(),
            session_context_digest: hash("context"),
            key_registry_epoch: 0,
            authenticated_at: 100,
            expires_at: 200,
            authentication_body_digest: hash("body"),
            verification_assurance:
                AuthorityVerificationAssurance::ControlVerifiedEcdsaP256Sha256X962,
        };
        let wire = AuthenticatedAuthoritySessionWireV1::from(session);
        assert_eq!(
            wire.verification_assurance,
            AuthoritySessionVerificationAssuranceV1::ControlVerifiedEcdsaP256Sha256X962
        );
        // The P-256 assurance carries a new, distinct SCREAMING_SNAKE wire name; it
        // is never folded into the pre-existing Ed25519 label.
        let json = serde_json::to_value(&wire).unwrap();
        assert_eq!(
            json["verification_assurance"],
            serde_json::json!("CONTROL_VERIFIED_ECDSA_P256_SHA256_X962")
        );
    }

    fn unsigned_receipt() -> AuthorityAuthorizationReceiptV1 {
        AuthorityAuthorizationReceiptV1::new_for_broker_test(AuthorityAuthorizationReceiptCoreV1 {
            organism_id: "organism-1".to_string(),
            repo_id: "repo-1".to_string(),
            brain_id: "brain-1".to_string(),
            subject_id: "owner-1".to_string(),
            role: Role::Author,
            capability_id: "capability-1".to_string(),
            capability_kind: Some(CapabilityKind::Human),
            verified_object_digest: hash("object"),
            mission_id: Some("mission-1".to_string()),
            mission_head_id: Some("head-1".to_string()),
            transport_session_id: "wire-1".to_string(),
            ingress_context_digest: hash("wire-context"),
            action: ActionId::new("mission.service.land").unwrap(),
            ingress: Ingress::Rest,
            complete_effects: BTreeSet::from([
                Effect::MissionStateWrite,
                Effect::RuntimeStoreWrite,
                Effect::CoordinationRecord,
                Effect::SovereignMutation,
            ]),
            active_mode: ActiveMode::HumanGated,
            constitution_digest: hash("constitution"),
            constitution_epoch: 1,
            autonomy_epoch: 0,
            protected_epoch_at_decision: 3,
            policy_registry_digest: hash("policy"),
            exact_policy_tuple: ReachablePolicyTupleV1 {
                ingress: Ingress::Rest,
                action: ActionId::new("mission.service.land").unwrap(),
                active_mode: ActiveMode::HumanGated,
                subject_id: "owner-1".to_string(),
                authority_variant: AuthorityVariant::Human,
                applicable_grant_id: None,
                applicable_tier: None,
                risk_class: RiskClass::Critical,
            },
            authority_decision_digest: Some(hash("decision")),
            autonomy_admission_receipt_digest: None,
            autonomy_committed_state_digest: None,
            autonomy_protected_root_digest: None,
            authority: AuthorizationAuthorityV1::Positive {
                variant: AuthorityVariant::Human,
                assurance: AuthorityVerificationAssurance::SoftwareTestOnlyNotProven,
            },
            authority_body_digest: hash("authority-body"),
            replay_sequence: 2,
            journal_sequence: 3,
            journal_root_digest: hash("journal"),
            protected_epoch: 3,
            authorized_at: 100,
            expires_at: 200,
        })
    }

    #[test]
    fn authorization_receipt_signature_covers_non_circular_body_and_signer_metadata() {
        let crypto = SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(
            b"authorization-receipt-signature-test-only",
        );
        let mut receipt = unsigned_receipt();
        sign_authorization_receipt(&mut receipt, &crypto).unwrap();
        verify_authorization_receipt(&receipt, &crypto).unwrap();

        let mut body_tamper = receipt.clone();
        body_tamper.core.verified_object_digest = hash("tampered-object");
        assert_eq!(
            verify_authorization_receipt(&body_tamper, &crypto)
                .unwrap_err()
                .code(),
            "authorization_receipt_binding_mismatch"
        );

        let mut signature_tamper = receipt;
        signature_tamper.signature = OpaqueSignature::new("tampered-signature");
        assert_eq!(
            verify_authorization_receipt(&signature_tamper, &crypto)
                .unwrap_err()
                .code(),
            "authorization_receipt_signature_invalid"
        );
    }
}
