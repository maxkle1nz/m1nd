use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{digest_canonical, CanonicalError, OpaqueSignature};

pub const CAUSAL_ENVELOPE_SCHEMA: &str = "m1nd-causal-envelope-v1";
pub const PAYLOAD_DIGEST_DOMAIN: &str = "m1nd-causal-payload-v1";
pub const REPLAY_KEY_DOMAIN: &str = "m1nd-causal-replay-key-v1";
pub const DEFAULT_CLOCK_SKEW_MS: u64 = 30_000;

/// Event class is validation context, not an extra wire field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventClass {
    Observation,
    Authenticated,
    AuthorizedMutation,
    MissionTransition,
    DelegatedMissionTransition,
    ReceiptMutation,
    InternalJournaled,
    InternalMissionTransition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventClassRequirements {
    pub signature: bool,
    pub capability: bool,
    pub mission_id: bool,
    pub mission_head_id: bool,
    pub delegation_id: bool,
    pub block_id: bool,
    pub receipt_id: bool,
    pub expiry: bool,
    pub journal_binding: bool,
    pub forbid_signature: bool,
    pub forbid_capability: bool,
}

impl EventClass {
    /// Normative G1 event-class matrix for conditional PRD 6.2 fields.
    pub const fn requirements(self) -> EventClassRequirements {
        match self {
            Self::Observation => EventClassRequirements::none(),
            Self::Authenticated => EventClassRequirements {
                signature: true,
                ..EventClassRequirements::none()
            },
            Self::AuthorizedMutation => EventClassRequirements {
                signature: true,
                capability: true,
                expiry: true,
                ..EventClassRequirements::none()
            },
            Self::MissionTransition => EventClassRequirements {
                signature: true,
                capability: true,
                mission_id: true,
                mission_head_id: true,
                expiry: true,
                ..EventClassRequirements::none()
            },
            Self::DelegatedMissionTransition => EventClassRequirements {
                signature: true,
                capability: true,
                mission_id: true,
                mission_head_id: true,
                delegation_id: true,
                expiry: true,
                ..EventClassRequirements::none()
            },
            Self::ReceiptMutation => EventClassRequirements {
                signature: true,
                capability: true,
                mission_id: true,
                mission_head_id: true,
                block_id: true,
                receipt_id: true,
                expiry: true,
                ..EventClassRequirements::none()
            },
            Self::InternalJournaled => EventClassRequirements {
                journal_binding: true,
                forbid_signature: true,
                forbid_capability: true,
                ..EventClassRequirements::none()
            },
            Self::InternalMissionTransition => EventClassRequirements {
                mission_id: true,
                mission_head_id: true,
                journal_binding: true,
                forbid_signature: true,
                forbid_capability: true,
                ..EventClassRequirements::none()
            },
        }
    }
}

impl EventClassRequirements {
    const fn none() -> Self {
        Self {
            signature: false,
            capability: false,
            mission_id: false,
            mission_head_id: false,
            delegation_id: false,
            block_id: false,
            receipt_id: false,
            expiry: false,
            journal_binding: false,
            forbid_signature: false,
            forbid_capability: false,
        }
    }
}

/// Exact PRD 6.2 wire fields. Conditional fields stay optional in the wire and
/// become mandatory through [`EventClass::requirements`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CausalEnvelopeV1 {
    pub schema: String,
    pub event_id: String,
    pub organism_id: String,
    pub brain_id: String,
    pub actor_id: String,
    pub actor_kind: String,
    pub issuer: String,
    pub key_id: Option<String>,
    pub algorithm: Option<String>,
    pub capability_id: Option<String>,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
    pub delegation_id: Option<String>,
    pub block_id: Option<String>,
    pub receipt_id: Option<String>,
    pub presence_id: Option<String>,
    pub graph_generation: u64,
    pub store_version: Option<u64>,
    pub target_digest: Option<String>,
    pub causation_id: Option<String>,
    pub correlation_id: String,
    pub issued_at: u64,
    pub expires_at: Option<u64>,
    pub payload_digest: String,
    pub signature: Option<OpaqueSignature>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegrityDisposition {
    UnsignedObservation,
    OpaqueSignaturePresentUnverified,
    JournalOrCheckpointBindingUnverified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvelopeValidationContext {
    pub now_ms: u64,
    pub max_future_clock_skew_ms: u64,
}

impl EnvelopeValidationContext {
    pub const fn at(now_ms: u64) -> Self {
        Self {
            now_ms,
            max_future_clock_skew_ms: DEFAULT_CLOCK_SKEW_MS,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvelopeValidation {
    pub event_class: EventClass,
    pub replay_key: String,
    pub integrity: IntegrityDisposition,
}

#[derive(Debug, Error)]
pub enum EnvelopeError {
    #[error("unsupported causal envelope schema '{actual}'")]
    Schema { actual: String },
    #[error("required field '{field}' is empty")]
    EmptyRequired { field: &'static str },
    #[error("event class {event_class:?} requires field '{field}'")]
    MissingForClass {
        event_class: EventClass,
        field: &'static str,
    },
    #[error("event class {event_class:?} forbids field '{field}'")]
    ForbiddenForClass {
        event_class: EventClass,
        field: &'static str,
    },
    #[error("mission_id and mission_head_id must be present together")]
    IncompleteMissionBinding,
    #[error("signature requires non-empty key_id and algorithm")]
    IncompleteSignatureBinding,
    #[error("capability_id without a signature is not an authenticated capability binding")]
    UnsignedCapability,
    #[error("expires_at ({expires_at}) must be later than issued_at ({issued_at})")]
    InvalidExpiryOrder { issued_at: u64, expires_at: u64 },
    #[error("envelope expired at {expires_at}; validation time is {now_ms}")]
    Expired { expires_at: u64, now_ms: u64 },
    #[error("issued_at {issued_at} exceeds permitted future time {latest_allowed}")]
    IssuedInFuture { issued_at: u64, latest_allowed: u64 },
    #[error("payload digest mismatch: expected {expected}, observed {observed}")]
    PayloadDigestMismatch { expected: String, observed: String },
    #[error(transparent)]
    Canonical(#[from] CanonicalError),
}

impl CausalEnvelopeV1 {
    pub fn payload_digest_for<T: Serialize + ?Sized>(
        payload: &T,
    ) -> Result<String, CanonicalError> {
        digest_canonical(PAYLOAD_DIGEST_DOMAIN, payload)
    }

    pub fn validate<T: Serialize + ?Sized>(
        &self,
        event_class: EventClass,
        payload: &T,
        now_ms: u64,
    ) -> Result<EnvelopeValidation, EnvelopeError> {
        self.validate_with_context(event_class, payload, EnvelopeValidationContext::at(now_ms))
    }

    /// Validate schema, event-class requirements, time window, and canonical
    /// payload binding. Signature bytes remain explicitly unverified.
    pub fn validate_with_context<T: Serialize + ?Sized>(
        &self,
        event_class: EventClass,
        payload: &T,
        context: EnvelopeValidationContext,
    ) -> Result<EnvelopeValidation, EnvelopeError> {
        if self.schema != CAUSAL_ENVELOPE_SCHEMA {
            return Err(EnvelopeError::Schema {
                actual: self.schema.clone(),
            });
        }

        for (field, value) in [
            ("event_id", self.event_id.as_str()),
            ("organism_id", self.organism_id.as_str()),
            ("brain_id", self.brain_id.as_str()),
            ("actor_id", self.actor_id.as_str()),
            ("actor_kind", self.actor_kind.as_str()),
            ("issuer", self.issuer.as_str()),
            ("correlation_id", self.correlation_id.as_str()),
            ("payload_digest", self.payload_digest.as_str()),
        ] {
            if value.is_empty() {
                return Err(EnvelopeError::EmptyRequired { field });
            }
        }

        validate_optional_non_empty("key_id", self.key_id.as_deref())?;
        validate_optional_non_empty("algorithm", self.algorithm.as_deref())?;
        validate_optional_non_empty("capability_id", self.capability_id.as_deref())?;
        validate_optional_non_empty("mission_id", self.mission_id.as_deref())?;
        validate_optional_non_empty("mission_head_id", self.mission_head_id.as_deref())?;
        validate_optional_non_empty("delegation_id", self.delegation_id.as_deref())?;
        validate_optional_non_empty("block_id", self.block_id.as_deref())?;
        validate_optional_non_empty("receipt_id", self.receipt_id.as_deref())?;
        validate_optional_non_empty("presence_id", self.presence_id.as_deref())?;
        validate_optional_non_empty("target_digest", self.target_digest.as_deref())?;
        validate_optional_non_empty("causation_id", self.causation_id.as_deref())?;

        if self.mission_id.is_some() != self.mission_head_id.is_some() {
            return Err(EnvelopeError::IncompleteMissionBinding);
        }
        if self.signature.is_some()
            && (self.key_id.as_deref().unwrap_or_default().is_empty()
                || self.algorithm.as_deref().unwrap_or_default().is_empty())
        {
            return Err(EnvelopeError::IncompleteSignatureBinding);
        }
        if self.capability_id.is_some() && self.signature.is_none() {
            return Err(EnvelopeError::UnsignedCapability);
        }
        if self
            .signature
            .as_ref()
            .is_some_and(OpaqueSignature::is_empty)
        {
            return Err(EnvelopeError::EmptyRequired { field: "signature" });
        }

        let requirements = event_class.requirements();
        require(
            event_class,
            "signature",
            requirements.signature,
            self.signature.is_some(),
        )?;
        require(
            event_class,
            "capability_id",
            requirements.capability,
            self.capability_id.is_some(),
        )?;
        require(
            event_class,
            "mission_id",
            requirements.mission_id,
            self.mission_id.is_some(),
        )?;
        require(
            event_class,
            "mission_head_id",
            requirements.mission_head_id,
            self.mission_head_id.is_some(),
        )?;
        require(
            event_class,
            "delegation_id",
            requirements.delegation_id,
            self.delegation_id.is_some(),
        )?;
        require(
            event_class,
            "block_id",
            requirements.block_id,
            self.block_id.is_some(),
        )?;
        require(
            event_class,
            "receipt_id",
            requirements.receipt_id,
            self.receipt_id.is_some(),
        )?;
        require(
            event_class,
            "expires_at",
            requirements.expiry,
            self.expires_at.is_some(),
        )?;
        if requirements.journal_binding {
            require(
                event_class,
                "target_digest",
                true,
                self.target_digest.is_some(),
            )?;
            require(
                event_class,
                "causation_id",
                true,
                self.causation_id.is_some(),
            )?;
        }
        forbid(
            event_class,
            "signature",
            requirements.forbid_signature,
            self.signature.is_some(),
        )?;
        forbid(
            event_class,
            "capability_id",
            requirements.forbid_capability,
            self.capability_id.is_some(),
        )?;

        let latest_allowed = context
            .now_ms
            .saturating_add(context.max_future_clock_skew_ms);
        if self.issued_at > latest_allowed {
            return Err(EnvelopeError::IssuedInFuture {
                issued_at: self.issued_at,
                latest_allowed,
            });
        }
        if let Some(expires_at) = self.expires_at {
            if expires_at <= self.issued_at {
                return Err(EnvelopeError::InvalidExpiryOrder {
                    issued_at: self.issued_at,
                    expires_at,
                });
            }
            // Authority windows are half-open: issued_at <= now < expires_at.
            // At the expiry instant the envelope is already invalid.
            if context.now_ms >= expires_at {
                return Err(EnvelopeError::Expired {
                    expires_at,
                    now_ms: context.now_ms,
                });
            }
        }

        let expected = Self::payload_digest_for(payload)?;
        if self.payload_digest != expected {
            return Err(EnvelopeError::PayloadDigestMismatch {
                expected,
                observed: self.payload_digest.clone(),
            });
        }

        let integrity = if requirements.journal_binding {
            IntegrityDisposition::JournalOrCheckpointBindingUnverified
        } else if self.signature.is_some() {
            IntegrityDisposition::OpaqueSignaturePresentUnverified
        } else {
            IntegrityDisposition::UnsignedObservation
        };

        Ok(EnvelopeValidation {
            event_class,
            replay_key: self.replay_key(event_class)?,
            integrity,
        })
    }

    pub fn replay_key(&self, event_class: EventClass) -> Result<String, CanonicalError> {
        let material = ReplayKeyMaterialV1 {
            event_class,
            event_id: &self.event_id,
            organism_id: &self.organism_id,
            brain_id: &self.brain_id,
            actor_id: &self.actor_id,
            issuer: &self.issuer,
            capability_id: self.capability_id.as_deref(),
            mission_id: self.mission_id.as_deref(),
            mission_head_id: self.mission_head_id.as_deref(),
            delegation_id: self.delegation_id.as_deref(),
            graph_generation: self.graph_generation,
            store_version: self.store_version,
            causation_id: self.causation_id.as_deref(),
            correlation_id: &self.correlation_id,
            issued_at: self.issued_at,
            expires_at: self.expires_at,
            payload_digest: &self.payload_digest,
        };
        digest_canonical(REPLAY_KEY_DOMAIN, &material)
    }
}

#[derive(Serialize)]
struct ReplayKeyMaterialV1<'a> {
    event_class: EventClass,
    event_id: &'a str,
    organism_id: &'a str,
    brain_id: &'a str,
    actor_id: &'a str,
    issuer: &'a str,
    capability_id: Option<&'a str>,
    mission_id: Option<&'a str>,
    mission_head_id: Option<&'a str>,
    delegation_id: Option<&'a str>,
    graph_generation: u64,
    store_version: Option<u64>,
    causation_id: Option<&'a str>,
    correlation_id: &'a str,
    issued_at: u64,
    expires_at: Option<u64>,
    payload_digest: &'a str,
}

fn validate_optional_non_empty(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), EnvelopeError> {
    if value.is_some_and(str::is_empty) {
        return Err(EnvelopeError::EmptyRequired { field });
    }
    Ok(())
}

fn require(
    event_class: EventClass,
    field: &'static str,
    required: bool,
    present: bool,
) -> Result<(), EnvelopeError> {
    if required && !present {
        return Err(EnvelopeError::MissingForClass { event_class, field });
    }
    Ok(())
}

fn forbid(
    event_class: EventClass,
    field: &'static str,
    forbidden: bool,
    present: bool,
) -> Result<(), EnvelopeError> {
    if forbidden && present {
        return Err(EnvelopeError::ForbiddenForClass { event_class, field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::collections::BTreeSet;

    fn payload() -> Value {
        json!({"action": "land", "scope": {"block": "block:one"}})
    }

    fn envelope() -> CausalEnvelopeV1 {
        CausalEnvelopeV1 {
            schema: CAUSAL_ENVELOPE_SCHEMA.into(),
            event_id: "event:one".into(),
            organism_id: "organism:one".into(),
            brain_id: "brain:one".into(),
            actor_id: "actor:one".into(),
            actor_kind: "AGENT".into(),
            issuer: "issuer:one".into(),
            key_id: Some("key:one".into()),
            algorithm: Some("opaque:g2-will-validate".into()),
            capability_id: Some("capability:one".into()),
            mission_id: Some("mission:one".into()),
            mission_head_id: Some("head:one".into()),
            delegation_id: Some("delegation:one".into()),
            block_id: Some("block:one".into()),
            receipt_id: Some("receipt:one".into()),
            presence_id: Some("presence:one".into()),
            graph_generation: 7,
            store_version: Some(9),
            target_digest: Some("target:digest".into()),
            causation_id: Some("cause:one".into()),
            correlation_id: "correlation:one".into(),
            issued_at: 1_000,
            expires_at: Some(2_000),
            payload_digest: CausalEnvelopeV1::payload_digest_for(&payload()).unwrap(),
            signature: Some(OpaqueSignature::new("opaque-signature")),
        }
    }

    #[test]
    fn receipt_mutation_requires_the_full_authorized_mission_binding() {
        let validation = envelope()
            .validate(EventClass::ReceiptMutation, &payload(), 1_500)
            .unwrap();
        assert_eq!(
            validation.integrity,
            IntegrityDisposition::OpaqueSignaturePresentUnverified
        );
    }

    #[test]
    fn wire_shape_matches_prd_6_2_exact_fields() {
        let value = serde_json::to_value(envelope()).unwrap();
        let actual: BTreeSet<String> = value.as_object().unwrap().keys().cloned().collect();
        let expected: BTreeSet<String> = [
            "schema",
            "event_id",
            "organism_id",
            "brain_id",
            "actor_id",
            "actor_kind",
            "issuer",
            "key_id",
            "algorithm",
            "capability_id",
            "mission_id",
            "mission_head_id",
            "delegation_id",
            "block_id",
            "receipt_id",
            "presence_id",
            "graph_generation",
            "store_version",
            "target_digest",
            "causation_id",
            "correlation_id",
            "issued_at",
            "expires_at",
            "payload_digest",
            "signature",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn event_class_requirement_matrix_is_explicit_and_fail_closed() {
        let observation = EventClass::Observation.requirements();
        assert!(!observation.signature);
        assert!(!observation.capability);
        assert!(!observation.journal_binding);

        let authenticated = EventClass::Authenticated.requirements();
        assert!(authenticated.signature);
        assert!(!authenticated.capability);

        let authorized = EventClass::AuthorizedMutation.requirements();
        assert!(authorized.signature && authorized.capability && authorized.expiry);

        let mission = EventClass::MissionTransition.requirements();
        assert!(mission.signature && mission.capability && mission.expiry);
        assert!(mission.mission_id && mission.mission_head_id);

        let delegated = EventClass::DelegatedMissionTransition.requirements();
        assert!(delegated.mission_id && delegated.mission_head_id && delegated.delegation_id);

        let receipt = EventClass::ReceiptMutation.requirements();
        assert!(receipt.mission_id && receipt.mission_head_id);
        assert!(receipt.block_id && receipt.receipt_id);

        let internal = EventClass::InternalJournaled.requirements();
        assert!(internal.journal_binding);
        assert!(internal.forbid_signature && internal.forbid_capability);

        let internal_mission = EventClass::InternalMissionTransition.requirements();
        assert!(internal_mission.journal_binding);
        assert!(internal_mission.mission_id && internal_mission.mission_head_id);
        assert!(internal_mission.forbid_signature && internal_mission.forbid_capability);
    }

    #[test]
    fn missing_signature_capability_and_mission_bindings_fail_closed() {
        type EnvelopeMutation = (&'static str, Box<dyn Fn(&mut CausalEnvelopeV1)>);

        let mutations: Vec<EnvelopeMutation> = vec![
            ("signature", Box::new(|env| env.signature = None)),
            ("capability_id", Box::new(|env| env.capability_id = None)),
            ("mission_id", Box::new(|env| env.mission_id = None)),
            (
                "mission_head_id",
                Box::new(|env| env.mission_head_id = None),
            ),
            ("block_id", Box::new(|env| env.block_id = None)),
            ("receipt_id", Box::new(|env| env.receipt_id = None)),
        ];

        for (field, mutate) in mutations {
            let mut env = envelope();
            mutate(&mut env);
            let error = env
                .validate(EventClass::ReceiptMutation, &payload(), 1_500)
                .unwrap_err();
            let text = error.to_string();
            assert!(
                text.contains(field) || text.contains("mission_id and mission_head_id"),
                "field={field}, error={text}"
            );
        }
    }

    #[test]
    fn delegated_transition_requires_delegation() {
        let mut env = envelope();
        env.delegation_id = None;
        assert!(matches!(
            env.validate(EventClass::DelegatedMissionTransition, &payload(), 1_500),
            Err(EnvelopeError::MissingForClass {
                field: "delegation_id",
                ..
            })
        ));
    }

    #[test]
    fn capability_without_signature_is_rejected_even_for_observation() {
        let mut env = envelope();
        env.signature = None;
        assert!(matches!(
            env.validate(EventClass::Observation, &payload(), 1_500),
            Err(EnvelopeError::UnsignedCapability)
        ));
    }

    #[test]
    fn signature_without_key_or_algorithm_is_rejected() {
        let mut no_key = envelope();
        no_key.key_id = None;
        assert!(matches!(
            no_key.validate(EventClass::Authenticated, &payload(), 1_500),
            Err(EnvelopeError::IncompleteSignatureBinding)
        ));

        let mut no_algorithm = envelope();
        no_algorithm.algorithm = None;
        assert!(matches!(
            no_algorithm.validate(EventClass::Authenticated, &payload(), 1_500),
            Err(EnvelopeError::IncompleteSignatureBinding)
        ));
    }

    #[test]
    fn authorized_classes_require_expiry() {
        let mut env = envelope();
        env.expires_at = None;
        assert!(matches!(
            env.validate(EventClass::AuthorizedMutation, &payload(), 1_500),
            Err(EnvelopeError::MissingForClass {
                field: "expires_at",
                ..
            })
        ));
    }

    #[test]
    fn expired_envelope_is_rejected() {
        let env = envelope();
        assert!(matches!(
            env.validate(EventClass::AuthorizedMutation, &payload(), 2_000),
            Err(EnvelopeError::Expired { .. })
        ));
    }

    #[test]
    fn invalid_expiry_order_is_rejected() {
        let mut env = envelope();
        env.expires_at = Some(env.issued_at);
        assert!(matches!(
            env.validate(EventClass::AuthorizedMutation, &payload(), 1_000),
            Err(EnvelopeError::InvalidExpiryOrder { .. })
        ));
    }

    #[test]
    fn excessive_future_issued_at_is_rejected() {
        let mut env = envelope();
        env.issued_at = 50_000;
        env.expires_at = Some(60_000);
        let context = EnvelopeValidationContext {
            now_ms: 1_000,
            max_future_clock_skew_ms: 100,
        };
        assert!(matches!(
            env.validate_with_context(EventClass::AuthorizedMutation, &payload(), context),
            Err(EnvelopeError::IssuedInFuture { .. })
        ));
    }

    #[test]
    fn changed_payload_is_digest_drift() {
        let env = envelope();
        let changed = json!({"action": "archive", "scope": {"block": "block:one"}});
        assert!(matches!(
            env.validate(EventClass::ReceiptMutation, &changed, 1_500),
            Err(EnvelopeError::PayloadDigestMismatch { .. })
        ));
    }

    #[test]
    fn payload_object_order_does_not_change_digest() {
        let first = json!({"b": 2, "a": {"d": 4, "c": 3}});
        let second = json!({"a": {"c": 3, "d": 4}, "b": 2});
        assert_eq!(
            CausalEnvelopeV1::payload_digest_for(&first).unwrap(),
            CausalEnvelopeV1::payload_digest_for(&second).unwrap()
        );
    }

    #[test]
    fn replay_key_binds_identity_capability_head_and_payload() {
        let env = envelope();
        let baseline = env.replay_key(EventClass::ReceiptMutation).unwrap();

        let mut variants = Vec::new();
        let mut changed_event = env.clone();
        changed_event.event_id = "event:two".into();
        variants.push(changed_event);
        let mut changed_capability = env.clone();
        changed_capability.capability_id = Some("capability:two".into());
        variants.push(changed_capability);
        let mut changed_head = env.clone();
        changed_head.mission_head_id = Some("head:two".into());
        variants.push(changed_head);
        let mut changed_payload = env.clone();
        changed_payload.payload_digest = "changed-payload-digest".into();
        variants.push(changed_payload);

        for variant in variants {
            assert_ne!(
                variant.replay_key(EventClass::ReceiptMutation).unwrap(),
                baseline
            );
        }
        assert_ne!(
            env.replay_key(EventClass::MissionTransition).unwrap(),
            baseline
        );
    }

    #[test]
    fn internal_journal_event_uses_no_fake_signature_or_capability() {
        let mut env = envelope();
        env.key_id = None;
        env.algorithm = None;
        env.signature = None;
        env.capability_id = None;
        env.mission_id = None;
        env.mission_head_id = None;
        env.delegation_id = None;
        env.block_id = None;
        env.receipt_id = None;
        env.expires_at = None;

        let validation = env
            .validate(EventClass::InternalJournaled, &payload(), 1_500)
            .unwrap();
        assert_eq!(
            validation.integrity,
            IntegrityDisposition::JournalOrCheckpointBindingUnverified
        );
    }

    #[test]
    fn internal_journal_event_requires_digest_and_causation_binding() {
        let mut env = envelope();
        env.key_id = None;
        env.algorithm = None;
        env.signature = None;
        env.capability_id = None;
        env.mission_id = None;
        env.mission_head_id = None;
        env.expires_at = None;
        env.target_digest = None;

        assert!(matches!(
            env.validate(EventClass::InternalJournaled, &payload(), 1_500),
            Err(EnvelopeError::MissingForClass {
                field: "target_digest",
                ..
            })
        ));
    }

    #[test]
    fn internal_class_forbids_opaque_authority_claims() {
        let env = envelope();
        assert!(matches!(
            env.validate(EventClass::InternalJournaled, &payload(), 1_500),
            Err(EnvelopeError::ForbiddenForClass {
                field: "signature",
                ..
            })
        ));
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let mut value = serde_json::to_value(envelope()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("invented".into(), json!(true));
        assert!(serde_json::from_value::<CausalEnvelopeV1>(value).is_err());
    }

    #[test]
    fn validation_never_claims_signature_verified() {
        let validation = envelope()
            .validate(EventClass::Authenticated, &payload(), 1_500)
            .unwrap();
        assert_eq!(
            validation.integrity,
            IntegrityDisposition::OpaqueSignaturePresentUnverified
        );
    }
}
