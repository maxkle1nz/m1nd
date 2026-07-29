use std::collections::BTreeMap;

use m1nd_control::{
    sign_capability, sign_human_approval, sign_owner_challenge, verify_capability,
    verify_capability_once, verify_human_approval, verify_owner_challenge, ActionId, ActiveMode,
    AuthorityCapabilityV1, AuthorityCryptoError, AuthoritySigner, AuthoritySignerError,
    AuthorityVariant, CapabilityVerificationContext, ChallengeVerificationContext,
    CryptographicIntegrity, HumanApprovalV1, HumanKeyRegistryV1, HumanKeyV1, IdentityStatus,
    MemoryReplayLedger, OpaqueSignature, OwnerChallengeV1, OwnerIdentityV1,
    VerificationKeyRegistryV1, VerificationKeyV1, AUTHORITY_CAPABILITY_SCHEMA,
    ECDSA_P256_SHA256_X962_ALGORITHM, OWNER_CHALLENGE_SIGNED_SCHEMA,
    VERIFICATION_KEY_REGISTRY_SCHEMA,
};
use p256::ecdsa::{signature::Signer as _, Signature as P256Signature, SigningKey, VerifyingKey};

const NOW: u64 = 2_000;
const SKEW: u64 = 100;

struct TestP256Signer {
    key_id: String,
    subject_id: String,
    signing_key: SigningKey,
    return_raw_signature: bool,
}

impl TestP256Signer {
    fn new(key_id: &str, subject_id: &str, seed: u8) -> Self {
        let secret = [seed; 32];
        Self {
            key_id: key_id.to_owned(),
            subject_id: subject_id.to_owned(),
            signing_key: SigningKey::from_bytes((&secret).into()).unwrap(),
            return_raw_signature: false,
        }
    }

    fn public_key_bytes(&self) -> Vec<u8> {
        VerifyingKey::from(&self.signing_key)
            .to_sec1_point(false)
            .as_bytes()
            .to_vec()
    }

    fn public_key_hex(&self) -> String {
        hex_lower(&self.public_key_bytes())
    }
}

impl AuthoritySigner for TestP256Signer {
    fn key_id(&self) -> &str {
        &self.key_id
    }

    fn subject_id(&self) -> &str {
        &self.subject_id
    }

    fn algorithm(&self) -> &str {
        ECDSA_P256_SHA256_X962_ALGORITHM
    }

    fn public_key_bytes(&self) -> Result<Vec<u8>, AuthoritySignerError> {
        Ok(self.public_key_bytes())
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, AuthoritySignerError> {
        let signature: P256Signature = self.signing_key.sign(message);
        if self.return_raw_signature {
            Ok(signature.to_bytes().to_vec())
        } else {
            Ok(signature.to_der().as_bytes().to_vec())
        }
    }
}

struct Fixtures {
    owner_signer: TestP256Signer,
    human_signer: TestP256Signer,
    owner: OwnerIdentityV1,
    human_keys: HumanKeyRegistryV1,
    verification_keys: VerificationKeyRegistryV1,
}

impl Fixtures {
    fn new() -> Self {
        let owner_signer = TestP256Signer::new("owner-p256-1", "owner-1", 7);
        let human_signer = TestP256Signer::new("human-p256-1", "human-1", 9);
        let owner = OwnerIdentityV1 {
            owner_id: "owner-1".to_owned(),
            key_id: owner_signer.key_id.clone(),
            non_exportable_public_key: owner_signer.public_key_hex(),
            pinned_trust_anchor: "test-p256-trust-anchor".to_owned(),
            protected_latest_epoch: 7,
        };
        let human_keys = HumanKeyRegistryV1 {
            owner_id: owner.owner_id.clone(),
            registry_epoch: 7,
            keys: BTreeMap::from([(
                human_signer.key_id.clone(),
                HumanKeyV1 {
                    key_id: human_signer.key_id.clone(),
                    subject_id: human_signer.subject_id.clone(),
                    platform: "test-platform".to_owned(),
                    public_key: human_signer.public_key_hex(),
                    attestation_class: "test-only-software-p256".to_owned(),
                    created_at: 500,
                    rotated_at: None,
                    revoked_at: None,
                    status: IdentityStatus::Active,
                },
            )]),
        };
        let verification_keys = VerificationKeyRegistryV1 {
            schema: VERIFICATION_KEY_REGISTRY_SCHEMA.to_owned(),
            registry_epoch: 7,
            keys: BTreeMap::from([
                verification_key(&owner_signer),
                verification_key(&human_signer),
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

    fn challenge(&self) -> OwnerChallengeV1 {
        let mut challenge = OwnerChallengeV1 {
            challenge_id: "challenge-p256-1".to_owned(),
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
            nonce: "challenge-nonce-p256-1".to_owned(),
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

    fn approval(&self, challenge: &OwnerChallengeV1) -> HumanApprovalV1 {
        let mut approval = HumanApprovalV1 {
            challenge_id: challenge.challenge_id.clone(),
            canonical_challenge_digest: challenge.canonical_digest().unwrap(),
            key_id: self.human_signer.key_id.clone(),
            subject_id: self.human_signer.subject_id.clone(),
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

    fn capability(&self) -> AuthorityCapabilityV1 {
        let mut capability = unsigned_capability();
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

#[test]
fn all_authority_artifacts_round_trip_with_canonical_p256() {
    let fixtures = Fixtures::new();
    let challenge = fixtures.challenge();
    let approval = fixtures.approval(&challenge);
    let capability = fixtures.capability();

    let verified_challenge = verify_owner_challenge(
        &challenge,
        &fixtures.owner,
        &fixtures.verification_keys,
        challenge_context(),
    )
    .unwrap();
    assert_eq!(
        verified_challenge.integrity,
        CryptographicIntegrity::VerifiedEcdsaP256Sha256X962
    );
    assert_eq!(
        verify_human_approval(
            &approval,
            &challenge,
            &fixtures.owner,
            &fixtures.human_keys,
            &fixtures.verification_keys,
            challenge_context(),
        )
        .unwrap()
        .integrity,
        CryptographicIntegrity::VerifiedEcdsaP256Sha256X962
    );
    assert_eq!(
        verify_capability(
            &capability,
            &fixtures.verification_keys,
            capability_context(),
        )
        .unwrap()
        .integrity,
        CryptographicIntegrity::VerifiedEcdsaP256Sha256X962
    );

    let signature = decode_hex(capability.signature.as_str());
    assert_eq!(signature.first(), Some(&0x30), "signature must be DER");
    assert_eq!(
        P256Signature::from_der(&signature)
            .unwrap()
            .to_der()
            .as_bytes(),
        signature
    );
}

#[test]
fn p256_replay_and_tamper_fail_closed() {
    let fixtures = Fixtures::new();
    let capability = fixtures.capability();
    let mut ledger = MemoryReplayLedger::default();
    verify_capability_once(
        &capability,
        &fixtures.verification_keys,
        capability_context(),
        &mut ledger,
    )
    .unwrap();
    assert!(verify_capability_once(
        &capability,
        &fixtures.verification_keys,
        capability_context(),
        &mut ledger,
    )
    .is_err());

    let mut tampered = capability.clone();
    tampered.payload_digest = "different-payload".to_owned();
    assert!(verify_capability(
        &tampered,
        &fixtures.verification_keys,
        CapabilityVerificationContext {
            expected_payload_digest: "different-payload",
            ..capability_context()
        },
    )
    .is_err());

    let mut malformed = capability;
    malformed.signature = OpaqueSignature::new("3000");
    assert!(matches!(
        verify_capability(
            &malformed,
            &fixtures.verification_keys,
            capability_context()
        ),
        Err(AuthorityCryptoError::SignatureEncoding)
    ));
}

#[test]
fn p256_rejects_compressed_keys_and_non_der_signer_output() {
    let fixtures = Fixtures::new();
    let mut compressed_registry = fixtures.verification_keys.clone();
    let key = compressed_registry
        .keys
        .get_mut(&fixtures.owner_signer.key_id)
        .unwrap();
    key.public_key = hex_lower(
        VerifyingKey::from(&fixtures.owner_signer.signing_key)
            .to_sec1_point(true)
            .as_bytes(),
    );
    assert!(matches!(
        compressed_registry.validate(NOW, SKEW),
        Err(AuthorityCryptoError::PublicKeyEncoding)
    ));

    let mut raw_signer = TestP256Signer::new("owner-p256-1", "owner-1", 7);
    raw_signer.return_raw_signature = true;
    let mut capability = unsigned_capability();
    assert!(matches!(
        sign_capability(
            &mut capability,
            &fixtures.verification_keys,
            &raw_signer,
            NOW,
            SKEW,
        ),
        Err(AuthorityCryptoError::SignatureEncoding)
    ));
}

fn verification_key(signer: &TestP256Signer) -> (String, VerificationKeyV1) {
    (
        signer.key_id.clone(),
        VerificationKeyV1 {
            key_id: signer.key_id.clone(),
            subject_id: signer.subject_id.clone(),
            algorithm: ECDSA_P256_SHA256_X962_ALGORITHM.to_owned(),
            public_key: signer.public_key_hex(),
            created_at: 400,
            activated_at: 500,
            expires_at: None,
            revoked_at: None,
            rotated_at: None,
            replacement_key_id: None,
            status: IdentityStatus::Active,
        },
    )
}

fn unsigned_capability() -> AuthorityCapabilityV1 {
    AuthorityCapabilityV1 {
        schema: AUTHORITY_CAPABILITY_SCHEMA.to_owned(),
        capability_id: "capability-p256-1".to_owned(),
        issuer_subject_id: "owner-1".to_owned(),
        issuer_key_id: "owner-p256-1".to_owned(),
        algorithm: ECDSA_P256_SHA256_X962_ALGORITHM.to_owned(),
        subject_id: "agent-executor-1".to_owned(),
        audience: "m1nd-runtime".to_owned(),
        organism_id: "organism-1".to_owned(),
        brain_id: "brain-1".to_owned(),
        mission_id: Some("mission-1".to_owned()),
        mission_head_id: Some("head-1".to_owned()),
        action: ActionId::new("mission.execute").unwrap(),
        authority_variant: AuthorityVariant::AgentQuorum,
        active_mode: ActiveMode::FullAutonomy,
        payload_digest: "payload-digest-1".to_owned(),
        policy_registry_digest: "policy-digest-1".to_owned(),
        constitution_digest: "constitution-digest-1".to_owned(),
        key_registry_epoch: 7,
        issued_at: 1_900,
        expires_at: 10_000,
        nonce: "capability-nonce-p256-1".to_owned(),
        signature: OpaqueSignature::new(""),
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
        expected_authority_variant: AuthorityVariant::AgentQuorum,
        expected_active_mode: ActiveMode::FullAutonomy,
    }
}

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).unwrap();
            u8::from_str_radix(pair, 16).unwrap()
        })
        .collect()
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
