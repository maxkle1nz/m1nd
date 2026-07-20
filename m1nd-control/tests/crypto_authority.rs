use std::collections::BTreeMap;

use ed25519_dalek::{Signer as _, SigningKey};
use m1nd_control::{
    sign_capability, sign_human_approval, sign_owner_challenge, verify_capability,
    verify_capability_once, verify_human_approval, verify_human_approval_once,
    verify_owner_challenge, verify_owner_challenge_once, ActionId, ActiveMode,
    AuthorityCapabilityV1, AuthorityCryptoError, AuthoritySigner, AuthoritySignerError,
    AuthorityVariant, CapabilityVerificationContext, ChallengeVerificationContext,
    CryptographicIntegrity, HumanApprovalV1, HumanKeyRegistryV1, HumanKeyV1, IdentityStatus,
    MemoryReplayLedger, OpaqueSignature, OwnerChallengeV1, OwnerIdentityV1, PersistentReplayLedger,
    ReplayLedgerError, VerificationKeyRegistryV1, VerificationKeyV1, VerifiedArtifact,
    AUTHORITY_CAPABILITY_SCHEMA, DEFAULT_AUTHORITY_CLOCK_SKEW_MS, ED25519_ALGORITHM,
    OWNER_CHALLENGE_SIGNED_SCHEMA, VERIFICATION_KEY_REGISTRY_SCHEMA,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

const NOW: u64 = 2_000;
const SKEW: u64 = 100;

struct TestEd25519Signer {
    key_id: String,
    subject_id: String,
    signing_key: SigningKey,
}

impl TestEd25519Signer {
    fn new(key_id: &str, subject_id: &str, seed: u8) -> Self {
        Self {
            key_id: key_id.to_owned(),
            subject_id: subject_id.to_owned(),
            signing_key: SigningKey::from_bytes(&[seed; 32]),
        }
    }

    fn public_key_hex(&self) -> String {
        hex_lower(&self.signing_key.verifying_key().to_bytes())
    }
}

impl AuthoritySigner for TestEd25519Signer {
    fn key_id(&self) -> &str {
        &self.key_id
    }

    fn subject_id(&self) -> &str {
        &self.subject_id
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

struct Fixtures {
    owner_signer: TestEd25519Signer,
    human_signer: TestEd25519Signer,
    owner: OwnerIdentityV1,
    human_keys: HumanKeyRegistryV1,
    verification_keys: VerificationKeyRegistryV1,
}

impl Fixtures {
    fn new() -> Self {
        let owner_signer = TestEd25519Signer::new("owner-key-1", "owner-1", 7);
        let human_signer = TestEd25519Signer::new("human-key-1", "human-1", 9);
        let owner = OwnerIdentityV1 {
            owner_id: "owner-1".to_owned(),
            key_id: "owner-key-1".to_owned(),
            non_exportable_public_key: owner_signer.public_key_hex(),
            pinned_trust_anchor: "trust-anchor-1".to_owned(),
            protected_latest_epoch: 7,
        };
        let human_key = HumanKeyV1 {
            key_id: "human-key-1".to_owned(),
            subject_id: "human-1".to_owned(),
            platform: "test-platform".to_owned(),
            public_key: human_signer.public_key_hex(),
            attestation_class: "test-only-software-key".to_owned(),
            created_at: 500,
            rotated_at: None,
            revoked_at: None,
            status: IdentityStatus::Active,
        };
        let human_keys = HumanKeyRegistryV1 {
            owner_id: "owner-1".to_owned(),
            registry_epoch: 7,
            keys: BTreeMap::from([("human-key-1".to_owned(), human_key)]),
        };
        let verification_keys = VerificationKeyRegistryV1 {
            schema: VERIFICATION_KEY_REGISTRY_SCHEMA.to_owned(),
            registry_epoch: 7,
            keys: BTreeMap::from([
                (
                    "owner-key-1".to_owned(),
                    verification_key(&owner_signer, 500),
                ),
                (
                    "human-key-1".to_owned(),
                    verification_key(&human_signer, 500),
                ),
            ]),
        };
        Self {
            owner_signer,
            human_signer,
            owner,
            human_keys,
            verification_keys,
        }
    }

    fn signed_challenge(&self) -> OwnerChallengeV1 {
        let mut challenge = OwnerChallengeV1 {
            challenge_id: "challenge-1".to_owned(),
            intent_digest: "intent-digest-1".to_owned(),
            intent_core_ref: "intent-core-1".to_owned(),
            intent_canonicalization_version: "m1nd-canonical-json-v1".to_owned(),
            organism_id: "organism-1".to_owned(),
            repo_id: "repo-1".to_owned(),
            issuer_subject_id: "owner-1".to_owned(),
            decision_subject_id: "human-1".to_owned(),
            caller_subject_id: "client-1".to_owned(),
            proposer_subject_id: "agent-proposer-1".to_owned(),
            executor_subject_id: Some("agent-executor-1".to_owned()),
            delegation_grant_digest: Some("delegation-digest-1".to_owned()),
            audience: "m1nd-owner".to_owned(),
            session_context_digest: "session-digest-1".to_owned(),
            action: ActionId::new("mission.land").unwrap(),
            required_authority_variant: AuthorityVariant::Human,
            action_policy_registry_digest: "policy-digest-1".to_owned(),
            classifier_decision_digest: "classifier-digest-1".to_owned(),
            active_mode: ActiveMode::HumanGated,
            constitution_digest: "constitution-digest-1".to_owned(),
            constitution_epoch: 3,
            autonomy_epoch: 4,
            brain_id: "brain-1".to_owned(),
            mission_id: Some("mission-1".to_owned()),
            mission_head_id: Some("head-1".to_owned()),
            block_id: Some("block-1".to_owned()),
            candidate_digest: Some("candidate-digest-1".to_owned()),
            risk_scope_digest: "risk-digest-1".to_owned(),
            expected_store_epoch: 5,
            expected_store_version: 6,
            expected_boundary_version: 7,
            expected_contract_version: 8,
            idempotency_key: "idempotency-1".to_owned(), // gitleaks:allow
            payload_digest: "payload-digest-1".to_owned(),
            canonical_summary: "land the verified candidate".to_owned(),
            nonce: "challenge-nonce-1".to_owned(),
            expires_at: 10_000,
            owner_signature: OpaqueSignature::new(""),
        };
        sign_owner_challenge(
            &mut challenge,
            &self.owner,
            &self.verification_keys,
            &self.owner_signer,
            NOW,
            SKEW,
        )
        .unwrap();
        challenge
    }

    fn signed_approval(&self, challenge: &OwnerChallengeV1) -> HumanApprovalV1 {
        let mut approval = HumanApprovalV1 {
            challenge_id: challenge.challenge_id.clone(),
            canonical_challenge_digest: challenge.canonical_digest().unwrap(),
            key_id: "human-key-1".to_owned(),
            subject_id: "human-1".to_owned(),
            user_presence_flags: "PRESENT|VERIFIED".to_owned(),
            counter: 42,
            signature: OpaqueSignature::new(""),
        };
        sign_human_approval(
            &mut approval,
            challenge,
            &self.owner,
            &self.human_keys,
            &self.verification_keys,
            &self.human_signer,
            challenge_context(),
        )
        .unwrap();
        approval
    }

    fn signed_capability(&self) -> AuthorityCapabilityV1 {
        let mut capability = AuthorityCapabilityV1 {
            schema: AUTHORITY_CAPABILITY_SCHEMA.to_owned(),
            capability_id: "capability-1".to_owned(),
            issuer_subject_id: "owner-1".to_owned(),
            issuer_key_id: "owner-key-1".to_owned(),
            algorithm: ED25519_ALGORITHM.to_owned(),
            subject_id: "agent-executor-1".to_owned(),
            audience: "m1nd-runtime".to_owned(),
            organism_id: "organism-1".to_owned(),
            brain_id: "brain-1".to_owned(),
            mission_id: Some("mission-1".to_owned()),
            mission_head_id: Some("head-1".to_owned()),
            action: ActionId::new("mission.execute").unwrap(),
            authority_variant: AuthorityVariant::Policy,
            active_mode: ActiveMode::FullAutonomy,
            payload_digest: "payload-digest-1".to_owned(),
            policy_registry_digest: "policy-digest-1".to_owned(),
            constitution_digest: "constitution-digest-1".to_owned(),
            key_registry_epoch: 7,
            issued_at: 1_900,
            expires_at: 10_000,
            nonce: "capability-nonce-1".to_owned(),
            signature: OpaqueSignature::new(""),
        };
        sign_capability(
            &mut capability,
            &self.verification_keys,
            &self.owner_signer,
            NOW,
            SKEW,
        )
        .unwrap();
        capability
    }
}

fn verification_key(signer: &TestEd25519Signer, activated_at: u64) -> VerificationKeyV1 {
    VerificationKeyV1 {
        key_id: signer.key_id.clone(),
        subject_id: signer.subject_id.clone(),
        algorithm: ED25519_ALGORITHM.to_owned(),
        public_key: signer.public_key_hex(),
        created_at: 400,
        activated_at,
        expires_at: None,
        revoked_at: None,
        rotated_at: None,
        replacement_key_id: None,
        status: IdentityStatus::Active,
    }
}

fn challenge_context() -> ChallengeVerificationContext<'static> {
    ChallengeVerificationContext {
        now_ms: NOW,
        max_future_clock_skew_ms: SKEW,
        expected_schema: OWNER_CHALLENGE_SIGNED_SCHEMA,
        expected_audience: "m1nd-owner",
        expected_decision_subject_id: "human-1",
        expected_payload_digest: "payload-digest-1",
        expected_brain_id: "brain-1",
        expected_mission_id: Some("mission-1"),
        expected_mission_head_id: Some("head-1"),
        expected_active_mode: ActiveMode::HumanGated,
    }
}

fn capability_context() -> CapabilityVerificationContext<'static> {
    CapabilityVerificationContext {
        now_ms: NOW,
        max_future_clock_skew_ms: SKEW,
        expected_schema: AUTHORITY_CAPABILITY_SCHEMA,
        expected_audience: "m1nd-runtime",
        expected_subject_id: "agent-executor-1",
        expected_payload_digest: "payload-digest-1",
        expected_organism_id: "organism-1",
        expected_brain_id: "brain-1",
        expected_mission_id: Some("mission-1"),
        expected_mission_head_id: Some("head-1"),
        expected_action: "mission.execute",
        expected_authority_variant: AuthorityVariant::Policy,
        expected_active_mode: ActiveMode::FullAutonomy,
    }
}

#[test]
fn challenge_approval_and_capability_cross_only_with_verified_ed25519() {
    let fixtures = Fixtures::new();
    let challenge = fixtures.signed_challenge();
    let approval = fixtures.signed_approval(&challenge);
    let capability = fixtures.signed_capability();

    let verified_challenge = verify_owner_challenge(
        &challenge,
        &fixtures.owner,
        &fixtures.verification_keys,
        challenge_context(),
    )
    .unwrap();
    assert_eq!(
        verified_challenge.artifact,
        VerifiedArtifact::OwnerChallenge
    );
    assert_eq!(
        verified_challenge.integrity,
        CryptographicIntegrity::VerifiedEd25519
    );

    let verified_approval = verify_human_approval(
        &approval,
        &challenge,
        &fixtures.owner,
        &fixtures.human_keys,
        &fixtures.verification_keys,
        challenge_context(),
    )
    .unwrap();
    assert_eq!(verified_approval.artifact, VerifiedArtifact::HumanApproval);

    let verified_capability = verify_capability(
        &capability,
        &fixtures.verification_keys,
        capability_context(),
    )
    .unwrap();
    assert_eq!(
        verified_capability.artifact,
        VerifiedArtifact::AuthorityCapability
    );
}

#[test]
fn every_owner_challenge_body_field_is_cryptographically_or_structurally_bound() {
    let fixtures = Fixtures::new();
    let challenge = fixtures.signed_challenge();
    let variants = tamper_every_body_field(&challenge, "owner_signature");
    let expected = serde_json::to_value(&challenge)
        .unwrap()
        .as_object()
        .unwrap()
        .len()
        - 1;
    assert_eq!(expected, 37, "review the signed challenge field inventory");
    assert_eq!(variants.len(), expected);

    for (field, tampered) in variants {
        assert!(
            verify_owner_challenge(
                &tampered,
                &fixtures.owner,
                &fixtures.verification_keys,
                challenge_context(),
            )
            .is_err(),
            "tampered challenge field was accepted: {field}"
        );
    }
}

#[test]
fn every_human_approval_body_field_is_cryptographically_or_structurally_bound() {
    let fixtures = Fixtures::new();
    let challenge = fixtures.signed_challenge();
    let approval = fixtures.signed_approval(&challenge);
    let variants = tamper_every_body_field(&approval, "signature");
    let expected = serde_json::to_value(&approval)
        .unwrap()
        .as_object()
        .unwrap()
        .len()
        - 1;
    assert_eq!(expected, 6, "review the signed approval field inventory");
    assert_eq!(variants.len(), expected);

    for (field, tampered) in variants {
        assert!(
            verify_human_approval(
                &tampered,
                &challenge,
                &fixtures.owner,
                &fixtures.human_keys,
                &fixtures.verification_keys,
                challenge_context(),
            )
            .is_err(),
            "tampered approval field was accepted: {field}"
        );
    }
}

#[test]
fn every_capability_body_field_is_cryptographically_or_context_bound() {
    let fixtures = Fixtures::new();
    let capability = fixtures.signed_capability();
    let variants = tamper_every_body_field(&capability, "signature");
    let expected = serde_json::to_value(&capability)
        .unwrap()
        .as_object()
        .unwrap()
        .len()
        - 1;
    assert_eq!(expected, 21, "review the signed capability field inventory");
    assert_eq!(variants.len(), expected);

    for (field, tampered) in variants {
        assert!(
            verify_capability(&tampered, &fixtures.verification_keys, capability_context(),)
                .is_err(),
            "tampered capability field was accepted: {field}"
        );
    }
}

#[test]
fn caller_context_rejects_wrong_schema_audience_subject_payload_head_brain_and_mode() {
    let fixtures = Fixtures::new();
    let challenge = fixtures.signed_challenge();

    let mut challenge_contexts = Vec::new();
    let mut context = challenge_context();
    context.expected_schema = "wrong-schema";
    challenge_contexts.push(context);
    let mut context = challenge_context();
    context.expected_audience = "wrong-audience";
    challenge_contexts.push(context);
    let mut context = challenge_context();
    context.expected_decision_subject_id = "wrong-subject";
    challenge_contexts.push(context);
    let mut context = challenge_context();
    context.expected_payload_digest = "wrong-payload";
    challenge_contexts.push(context);
    let mut context = challenge_context();
    context.expected_mission_head_id = Some("wrong-head");
    challenge_contexts.push(context);
    let mut context = challenge_context();
    context.expected_brain_id = "wrong-brain";
    challenge_contexts.push(context);
    let mut context = challenge_context();
    context.expected_active_mode = ActiveMode::PolicyAutonomous;
    challenge_contexts.push(context);
    for context in challenge_contexts {
        assert!(verify_owner_challenge(
            &challenge,
            &fixtures.owner,
            &fixtures.verification_keys,
            context
        )
        .is_err());
    }

    let capability = fixtures.signed_capability();
    let mut capability_contexts = Vec::new();
    let mut context = capability_context();
    context.expected_schema = "wrong-schema";
    capability_contexts.push(context);
    let mut context = capability_context();
    context.expected_audience = "wrong-audience";
    capability_contexts.push(context);
    let mut context = capability_context();
    context.expected_subject_id = "wrong-subject";
    capability_contexts.push(context);
    let mut context = capability_context();
    context.expected_payload_digest = "wrong-payload";
    capability_contexts.push(context);
    let mut context = capability_context();
    context.expected_mission_head_id = Some("wrong-head");
    capability_contexts.push(context);
    let mut context = capability_context();
    context.expected_brain_id = "wrong-brain";
    capability_contexts.push(context);
    let mut context = capability_context();
    context.expected_active_mode = ActiveMode::PolicyAutonomous;
    capability_contexts.push(context);
    for context in capability_contexts {
        assert!(verify_capability(&capability, &fixtures.verification_keys, context).is_err());
    }
}

#[test]
fn revoked_rotated_unknown_and_wrong_public_keys_fail_closed() {
    let fixtures = Fixtures::new();
    let challenge = fixtures.signed_challenge();

    let mut revoked = fixtures.verification_keys.clone();
    let owner_key = revoked.keys.get_mut("owner-key-1").unwrap();
    owner_key.status = IdentityStatus::Revoked;
    owner_key.revoked_at = Some(1_500);
    assert!(matches!(
        verify_owner_challenge(&challenge, &fixtures.owner, &revoked, challenge_context()),
        Err(AuthorityCryptoError::KeyRevoked { .. })
    ));

    let replacement_signer = TestEd25519Signer::new("owner-key-2", "owner-1", 11);
    let mut rotated = fixtures.verification_keys.clone();
    let owner_key = rotated.keys.get_mut("owner-key-1").unwrap();
    owner_key.status = IdentityStatus::Rotated;
    owner_key.rotated_at = Some(1_500);
    owner_key.replacement_key_id = Some("owner-key-2".to_owned());
    rotated.keys.insert(
        "owner-key-2".to_owned(),
        verification_key(&replacement_signer, 1_500),
    );
    assert!(matches!(
        verify_owner_challenge(&challenge, &fixtures.owner, &rotated, challenge_context()),
        Err(AuthorityCryptoError::KeyRotated { .. })
    ));

    let mut unknown_owner = fixtures.owner.clone();
    unknown_owner.key_id = "missing-key".to_owned();
    assert!(matches!(
        verify_owner_challenge(
            &challenge,
            &unknown_owner,
            &fixtures.verification_keys,
            challenge_context()
        ),
        Err(AuthorityCryptoError::StructuralIdentity(_))
            | Err(AuthorityCryptoError::KeyNotFound { .. })
    ));

    let mut wrong_public = fixtures.verification_keys.clone();
    wrong_public.keys.get_mut("owner-key-1").unwrap().public_key =
        fixtures.human_signer.public_key_hex();
    assert!(matches!(
        verify_owner_challenge(
            &challenge,
            &fixtures.owner,
            &wrong_public,
            challenge_context()
        ),
        Err(AuthorityCryptoError::PinnedPublicKeyMismatch { .. })
    ));
}

#[test]
fn human_approval_rejects_revoked_verification_key_and_wrong_signature_key() {
    let fixtures = Fixtures::new();
    let challenge = fixtures.signed_challenge();
    let approval = fixtures.signed_approval(&challenge);

    let mut revoked = fixtures.verification_keys.clone();
    let human_key = revoked.keys.get_mut("human-key-1").unwrap();
    human_key.status = IdentityStatus::Revoked;
    human_key.revoked_at = Some(1_500);
    assert!(matches!(
        verify_human_approval(
            &approval,
            &challenge,
            &fixtures.owner,
            &fixtures.human_keys,
            &revoked,
            challenge_context()
        ),
        Err(AuthorityCryptoError::KeyRevoked { .. })
    ));

    let mut wrong_signature = approval.clone();
    wrong_signature.signature = challenge.owner_signature.clone();
    assert!(matches!(
        verify_human_approval(
            &wrong_signature,
            &challenge,
            &fixtures.owner,
            &fixtures.human_keys,
            &fixtures.verification_keys,
            challenge_context()
        ),
        Err(AuthorityCryptoError::SignatureInvalid)
    ));
}

#[test]
fn malformed_and_tampered_signature_bytes_fail_closed() {
    let fixtures = Fixtures::new();
    let mut capability = fixtures.signed_capability();
    capability.signature = OpaqueSignature::new(capability.signature.as_str().to_uppercase());
    assert!(matches!(
        verify_capability(
            &capability,
            &fixtures.verification_keys,
            capability_context()
        ),
        Err(AuthorityCryptoError::SignatureEncoding)
    ));

    let mut capability = fixtures.signed_capability();
    let mut signature = capability.signature.as_str().to_owned().into_bytes();
    signature[0] = if signature[0] == b'0' { b'1' } else { b'0' };
    capability.signature = OpaqueSignature::new(String::from_utf8(signature).unwrap());
    assert!(matches!(
        verify_capability(
            &capability,
            &fixtures.verification_keys,
            capability_context()
        ),
        Err(AuthorityCryptoError::SignatureInvalid)
    ));
}

#[test]
fn capability_expiry_and_future_clock_skew_are_enforced() {
    let fixtures = Fixtures::new();

    let mut at_skew_boundary = fixtures.signed_capability();
    at_skew_boundary.issued_at = NOW + SKEW;
    sign_capability(
        &mut at_skew_boundary,
        &fixtures.verification_keys,
        &fixtures.owner_signer,
        NOW,
        SKEW,
    )
    .unwrap();
    verify_capability(
        &at_skew_boundary,
        &fixtures.verification_keys,
        capability_context(),
    )
    .unwrap();

    let mut beyond_skew = fixtures.signed_capability();
    beyond_skew.issued_at = NOW + SKEW + 1;
    sign_capability(
        &mut beyond_skew,
        &fixtures.verification_keys,
        &fixtures.owner_signer,
        NOW + SKEW + 1,
        SKEW,
    )
    .unwrap();
    assert!(matches!(
        verify_capability(
            &beyond_skew,
            &fixtures.verification_keys,
            capability_context()
        ),
        Err(AuthorityCryptoError::IssuedInFuture { .. })
    ));

    let mut expired = fixtures.signed_capability();
    expired.issued_at = 1_000;
    expired.expires_at = NOW;
    sign_capability(
        &mut expired,
        &fixtures.verification_keys,
        &fixtures.owner_signer,
        NOW - 1,
        SKEW,
    )
    .unwrap();
    assert!(matches!(
        verify_capability(&expired, &fixtures.verification_keys, capability_context()),
        Err(AuthorityCryptoError::Expired { .. })
    ));
}

#[test]
fn signer_trait_rejects_wrong_key_without_exposing_private_material() {
    let fixtures = Fixtures::new();
    let mut capability = fixtures.signed_capability();
    let wrong_signer = TestEd25519Signer::new("owner-key-1", "owner-1", 99);
    assert!(matches!(
        sign_capability(
            &mut capability,
            &fixtures.verification_keys,
            &wrong_signer,
            NOW,
            DEFAULT_AUTHORITY_CLOCK_SKEW_MS
        ),
        Err(AuthorityCryptoError::PinnedPublicKeyMismatch { .. })
    ));
}

#[test]
fn challenge_approval_and_capability_each_reject_second_consumption() {
    let fixtures = Fixtures::new();
    let challenge = fixtures.signed_challenge();
    let approval = fixtures.signed_approval(&challenge);
    let capability = fixtures.signed_capability();

    let mut challenge_ledger = MemoryReplayLedger::new();
    verify_owner_challenge_once(
        &challenge,
        &fixtures.owner,
        &fixtures.verification_keys,
        challenge_context(),
        &mut challenge_ledger,
    )
    .unwrap();
    assert!(matches!(
        verify_owner_challenge_once(
            &challenge,
            &fixtures.owner,
            &fixtures.verification_keys,
            challenge_context(),
            &mut challenge_ledger
        ),
        Err(AuthorityCryptoError::Replay(
            ReplayLedgerError::Replay { .. }
        ))
    ));

    let mut approval_ledger = MemoryReplayLedger::new();
    verify_human_approval_once(
        &approval,
        &challenge,
        &fixtures.owner,
        &fixtures.human_keys,
        &fixtures.verification_keys,
        challenge_context(),
        &mut approval_ledger,
    )
    .unwrap();
    assert!(matches!(
        verify_human_approval_once(
            &approval,
            &challenge,
            &fixtures.owner,
            &fixtures.human_keys,
            &fixtures.verification_keys,
            challenge_context(),
            &mut approval_ledger
        ),
        Err(AuthorityCryptoError::Replay(
            ReplayLedgerError::Replay { .. }
        ))
    ));

    let mut capability_ledger = MemoryReplayLedger::new();
    verify_capability_once(
        &capability,
        &fixtures.verification_keys,
        capability_context(),
        &mut capability_ledger,
    )
    .unwrap();
    assert!(matches!(
        verify_capability_once(
            &capability,
            &fixtures.verification_keys,
            capability_context(),
            &mut capability_ledger
        ),
        Err(AuthorityCryptoError::Replay(
            ReplayLedgerError::Replay { .. }
        ))
    ));
}

#[test]
fn capability_replay_remains_rejected_after_process_restart() {
    let fixtures = Fixtures::new();
    let capability = fixtures.signed_capability();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("authority-replay.jsonl");
    {
        let mut ledger = PersistentReplayLedger::open(&path).unwrap();
        verify_capability_once(
            &capability,
            &fixtures.verification_keys,
            capability_context(),
            &mut ledger,
        )
        .unwrap();
    }

    let mut restarted = PersistentReplayLedger::open(&path).unwrap();
    assert!(matches!(
        verify_capability_once(
            &capability,
            &fixtures.verification_keys,
            capability_context(),
            &mut restarted
        ),
        Err(AuthorityCryptoError::Replay(
            ReplayLedgerError::Replay { .. }
        ))
    ));
}

fn tamper_every_body_field<T>(record: &T, signature_field: &str) -> Vec<(String, T)>
where
    T: Serialize + DeserializeOwned,
{
    let value = serde_json::to_value(record).unwrap();
    let object = value.as_object().unwrap();
    object
        .keys()
        .filter(|field| field.as_str() != signature_field)
        .map(|field| {
            let mut changed = value.clone();
            let target = changed.as_object_mut().unwrap().get_mut(field).unwrap();
            tamper_value(field, target);
            let record = serde_json::from_value(changed).unwrap_or_else(|error| {
                panic!("tamper for field {field} did not deserialize: {error}")
            });
            (field.clone(), record)
        })
        .collect()
}

fn tamper_value(field: &str, value: &mut Value) {
    match (field, value) {
        ("required_authority_variant", Value::String(value)) => *value = "POLICY".to_owned(),
        ("authority_variant", Value::String(value)) => *value = "AGENT_QUORUM".to_owned(),
        ("active_mode", Value::String(value)) if value == "HUMAN_GATED" => {
            *value = "POLICY_AUTONOMOUS".to_owned();
        }
        ("active_mode", Value::String(value)) => *value = "POLICY_AUTONOMOUS".to_owned(),
        (_, Value::String(value)) => *value = format!("tampered-{value}"),
        (_, Value::Number(value)) => {
            *value = serde_json::Number::from(value.as_u64().unwrap().saturating_add(1));
        }
        (_, Value::Bool(value)) => *value = !*value,
        (_, target @ Value::Null) => {
            *target = Value::String("tampered-optional".to_owned());
        }
        (_, Value::Array(values)) => values.push(Value::String("tampered".to_owned())),
        (_, Value::Object(values)) => {
            values.insert("tampered".to_owned(), Value::Bool(true));
        }
    }
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
