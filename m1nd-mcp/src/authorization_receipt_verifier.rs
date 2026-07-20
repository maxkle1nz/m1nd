//! One-shot offline verification for G6 runtime authorization receipts.
//!
//! The binary mode using this module is intentionally smaller than an owner:
//! it consumes one bounded JSON request from stdin, uses the process system
//! clock, verifies one pinned Ed25519 key and receipt, emits one closed JSON
//! proof, and exits. It never opens a transport or initializes runtime state.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use m1nd_control::{
    digest_canonical, verify_authority_message_signature, CryptographicIntegrity,
    VerificationKeyRegistryV1, VerificationKeyV1, ED25519_ALGORITHM,
    VERIFICATION_KEY_REGISTRY_SCHEMA,
};
use serde::{Deserialize, Serialize};

use crate::authority_runtime::{
    AuthorityAuthorizationReceiptV1, AUTHORIZATION_RECEIPT_DIGEST_DOMAIN,
    AUTHORIZATION_RECEIPT_SCHEMA,
};
use crate::authority_transport::authorization_receipt_signature_message;

pub const AUTHORIZATION_RECEIPT_VERIFICATION_REQUEST_SCHEMA: &str =
    "m1nd-g6-authorization-receipt-verification-request-v1";
pub const AUTHORIZATION_RECEIPT_VERIFICATION_PROOF_SCHEMA: &str =
    "m1nd-g6-authorization-receipt-verification-proof-v1";
pub const MAX_AUTHORIZATION_RECEIPT_LIFETIME_MS: u64 = 5 * 60 * 1_000;
pub const MAX_FUTURE_CLOCK_SKEW_MS: u64 = 30_000;
pub const MAX_VERIFICATION_REQUEST_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuthorizationReceiptVerificationRequestV1 {
    schema: String,
    receipt: AuthorityAuthorizationReceiptV1,
    verification_key: VerificationKeyV1,
    max_future_clock_skew_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum AuthorizationReceiptVerificationStatusV1 {
    Verified,
    InvalidRequest,
    ReceiptBindingInvalid,
    ReceiptClockInvalid,
    KeyLifecycleInvalid,
    SignatureInvalid,
    SystemClockUnavailable,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct AuthorizationReceiptVerificationProofV1 {
    schema: String,
    status: AuthorizationReceiptVerificationStatusV1,
    checked_at_ms: u64,
    receipt_digest: String,
    issuer: String,
    key_id: String,
    algorithm: String,
    signature_verified: bool,
    clock_verified: bool,
    key_lifecycle_verified: bool,
}

impl AuthorizationReceiptVerificationProofV1 {
    fn empty(status: AuthorizationReceiptVerificationStatusV1, checked_at_ms: u64) -> Self {
        Self {
            schema: AUTHORIZATION_RECEIPT_VERIFICATION_PROOF_SCHEMA.to_owned(),
            status,
            checked_at_ms,
            receipt_digest: String::new(),
            issuer: String::new(),
            key_id: String::new(),
            algorithm: String::new(),
            signature_verified: false,
            clock_verified: false,
            key_lifecycle_verified: false,
        }
    }

    fn for_receipt(receipt: &AuthorityAuthorizationReceiptV1, checked_at_ms: u64) -> Self {
        Self {
            receipt_digest: receipt.receipt_digest.clone(),
            issuer: receipt.issuer.clone(),
            key_id: receipt.key_id.clone(),
            algorithm: receipt.algorithm.clone(),
            ..Self::empty(
                AuthorizationReceiptVerificationStatusV1::InvalidRequest,
                checked_at_ms,
            )
        }
    }

    fn verified(&self) -> bool {
        self.status == AuthorizationReceiptVerificationStatusV1::Verified
    }
}

fn verify_request_at(
    request: AuthorizationReceiptVerificationRequestV1,
    checked_at_ms: u64,
) -> AuthorizationReceiptVerificationProofV1 {
    let mut proof =
        AuthorizationReceiptVerificationProofV1::for_receipt(&request.receipt, checked_at_ms);

    if request.schema != AUTHORIZATION_RECEIPT_VERIFICATION_REQUEST_SCHEMA
        || request.max_future_clock_skew_ms > MAX_FUTURE_CLOCK_SKEW_MS
    {
        return proof;
    }

    let receipt = &request.receipt;
    let verification_key = &request.verification_key;
    if receipt.schema != AUTHORIZATION_RECEIPT_SCHEMA
        || receipt.issuer != verification_key.subject_id
        || receipt.key_id != verification_key.key_id
        || receipt.algorithm != verification_key.algorithm
        || receipt.algorithm != ED25519_ALGORITHM
    {
        proof.status = AuthorizationReceiptVerificationStatusV1::ReceiptBindingInvalid;
        return proof;
    }

    let expected_digest = match digest_canonical(AUTHORIZATION_RECEIPT_DIGEST_DOMAIN, &receipt.core)
    {
        Ok(digest) => digest,
        Err(_) => {
            proof.status = AuthorizationReceiptVerificationStatusV1::ReceiptBindingInvalid;
            return proof;
        }
    };
    if expected_digest != receipt.receipt_digest {
        proof.status = AuthorizationReceiptVerificationStatusV1::ReceiptBindingInvalid;
        return proof;
    }

    let latest_allowed = checked_at_ms.saturating_add(request.max_future_clock_skew_ms);
    let lifetime = receipt
        .core
        .expires_at
        .checked_sub(receipt.core.authorized_at);
    if lifetime
        .is_none_or(|lifetime| lifetime == 0 || lifetime > MAX_AUTHORIZATION_RECEIPT_LIFETIME_MS)
        || receipt.core.authorized_at > latest_allowed
        || checked_at_ms >= receipt.core.expires_at
    {
        proof.status = AuthorizationReceiptVerificationStatusV1::ReceiptClockInvalid;
        return proof;
    }
    proof.clock_verified = true;

    let registry = VerificationKeyRegistryV1 {
        schema: VERIFICATION_KEY_REGISTRY_SCHEMA.to_owned(),
        registry_epoch: 0,
        keys: BTreeMap::from([(verification_key.key_id.clone(), verification_key.clone())]),
    };
    if registry
        .resolve_active(
            &receipt.key_id,
            &receipt.issuer,
            receipt.core.authorized_at,
            request.max_future_clock_skew_ms,
        )
        .is_err()
    {
        proof.status = AuthorizationReceiptVerificationStatusV1::KeyLifecycleInvalid;
        return proof;
    }
    let active_key = match registry.resolve_active(
        &receipt.key_id,
        &receipt.issuer,
        checked_at_ms,
        request.max_future_clock_skew_ms,
    ) {
        Ok(key) => key,
        Err(_) => {
            proof.status = AuthorizationReceiptVerificationStatusV1::KeyLifecycleInvalid;
            return proof;
        }
    };
    proof.key_lifecycle_verified = true;

    let canonical_payload = match receipt.canonical_signature_payload() {
        Ok(payload) => payload,
        Err(_) => {
            proof.status = AuthorizationReceiptVerificationStatusV1::ReceiptBindingInvalid;
            return proof;
        }
    };
    let message = authorization_receipt_signature_message(&canonical_payload);
    match verify_authority_message_signature(&message, &receipt.signature, active_key) {
        Ok(CryptographicIntegrity::VerifiedEd25519) => {}
        Ok(_) | Err(_) => {
            proof.status = AuthorizationReceiptVerificationStatusV1::SignatureInvalid;
            return proof;
        }
    }

    proof.signature_verified = true;
    proof.status = AuthorizationReceiptVerificationStatusV1::Verified;
    proof
}

fn system_clock_ms() -> Option<u64> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(elapsed.as_millis()).ok()
}

fn read_request<R: Read>(reader: R) -> Result<AuthorizationReceiptVerificationRequestV1, ()> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_VERIFICATION_REQUEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.is_empty() || bytes.len() > MAX_VERIFICATION_REQUEST_BYTES {
        return Err(());
    }
    serde_json::from_slice(&bytes).map_err(|_| ())
}

fn write_proof<W: Write>(mut writer: W, proof: &AuthorizationReceiptVerificationProofV1) -> bool {
    serde_json::to_writer(&mut writer, proof).is_ok()
        && writer.write_all(b"\n").is_ok()
        && writer.flush().is_ok()
}

/// Run the isolated one-shot verifier over process stdin/stdout.
///
/// Returns `0` only for a fully verified proof. Every malformed, stale,
/// untrusted, or unverifiable input emits a closed refusal proof and returns
/// `2`.
pub fn run_authorization_receipt_verifier_stdio() -> i32 {
    let checked_at_ms = match system_clock_ms() {
        Some(value) => value,
        None => {
            let proof = AuthorizationReceiptVerificationProofV1::empty(
                AuthorizationReceiptVerificationStatusV1::SystemClockUnavailable,
                0,
            );
            let _ = write_proof(io::stdout().lock(), &proof);
            return 2;
        }
    };
    let proof = match read_request(io::stdin().lock()) {
        Ok(request) => verify_request_at(request, checked_at_ms),
        Err(()) => AuthorizationReceiptVerificationProofV1::empty(
            AuthorizationReceiptVerificationStatusV1::InvalidRequest,
            checked_at_ms,
        ),
    };
    let verified = proof.verified();
    if !write_proof(io::stdout().lock(), &proof) {
        return 2;
    }
    if verified {
        0
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ed25519_dalek::{Signer as _, SigningKey};
    use m1nd_control::{
        ActionId, ActiveMode, AuthorityVariant, CapabilityKind, Effect, IdentityStatus, Ingress,
        OpaqueSignature, ReachablePolicyTupleV1, RiskClass, Role,
    };

    use super::*;
    use crate::authority_runtime::{
        AuthorityAuthorizationReceiptCoreV1, AuthorityVerificationAssurance,
        AuthorizationAuthorityV1,
    };

    const NOW: u64 = 1_000_000;

    fn hex_lower(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn hash(label: &str) -> String {
        digest_canonical("m1nd-g6-offline-verifier-test-v1", &label).unwrap()
    }

    fn fixture_core(authorized_at: u64, expires_at: u64) -> AuthorityAuthorizationReceiptCoreV1 {
        AuthorityAuthorizationReceiptCoreV1 {
            organism_id: "organism-1".to_owned(),
            repo_id: "repo-alpha".to_owned(),
            brain_id: "brain-1".to_owned(),
            subject_id: "agent-1".to_owned(),
            role: Role::Author,
            capability_id: "capability-1".to_owned(),
            capability_kind: Some(CapabilityKind::Human),
            verified_object_digest: hash("object"),
            mission_id: Some("mission-1".to_owned()),
            mission_head_id: Some("head-1".to_owned()),
            transport_session_id: "transport-1".to_owned(),
            ingress_context_digest: hash("ingress"),
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
                subject_id: "agent-1".to_owned(),
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
                assurance: AuthorityVerificationAssurance::ControlVerifiedEd25519,
            },
            authority_body_digest: hash("authority-body"),
            replay_sequence: 2,
            journal_sequence: 3,
            journal_root_digest: hash("journal"),
            protected_epoch: 3,
            authorized_at,
            expires_at,
        }
    }

    fn fixture_request(
        authorized_at: u64,
        expires_at: u64,
    ) -> AuthorizationReceiptVerificationRequestV1 {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let verification_key = /* gitleaks:allow */ VerificationKeyV1 {
            key_id: "owner-key-1".to_owned(),
            subject_id: "owner-1".to_owned(),
            algorithm: ED25519_ALGORITHM.to_owned(),
            public_key: hex_lower(signing_key.verifying_key().as_bytes()),
            created_at: NOW - 10_000,
            activated_at: NOW - 9_000,
            expires_at: None,
            revoked_at: None,
            rotated_at: None,
            replacement_key_id: None,
            status: IdentityStatus::Active,
        };
        let core = fixture_core(authorized_at, expires_at);
        let receipt_digest = digest_canonical(AUTHORIZATION_RECEIPT_DIGEST_DOMAIN, &core).unwrap();
        let mut receipt = AuthorityAuthorizationReceiptV1 {
            schema: AUTHORIZATION_RECEIPT_SCHEMA.to_owned(),
            core,
            receipt_digest,
            issuer: verification_key.subject_id.clone(),
            key_id: verification_key.key_id.clone(),
            algorithm: verification_key.algorithm.clone(),
            signature: OpaqueSignature::new("pending"),
        };
        let canonical = receipt.canonical_signature_payload().unwrap();
        let message = authorization_receipt_signature_message(&canonical);
        receipt.signature = OpaqueSignature::new(hex_lower(&signing_key.sign(&message).to_bytes()));
        AuthorizationReceiptVerificationRequestV1 {
            schema: AUTHORIZATION_RECEIPT_VERIFICATION_REQUEST_SCHEMA.to_owned(),
            receipt,
            verification_key,
            max_future_clock_skew_ms: 1_000,
        }
    }

    #[test]
    fn verifies_exact_receipt_digest_signature_clock_and_active_key() {
        let proof = verify_request_at(fixture_request(NOW - 1_000, NOW + 1_000), NOW);
        assert_eq!(
            proof.status,
            AuthorizationReceiptVerificationStatusV1::Verified
        );
        assert!(proof.signature_verified);
        assert!(proof.clock_verified);
        assert!(proof.key_lifecycle_verified);
    }

    #[test]
    fn refuses_tampered_core_before_signature_claim() {
        let mut request = fixture_request(NOW - 1_000, NOW + 1_000);
        request.receipt.core.verified_object_digest = hash("tampered");
        let proof = verify_request_at(request, NOW);
        assert_eq!(
            proof.status,
            AuthorizationReceiptVerificationStatusV1::ReceiptBindingInvalid
        );
        assert!(!proof.signature_verified);
    }

    #[test]
    fn refuses_non_receipt_signature_framing() {
        let mut request = fixture_request(NOW - 1_000, NOW + 1_000);
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let canonical = request.receipt.canonical_signature_payload().unwrap();
        request.receipt.signature =
            OpaqueSignature::new(hex_lower(&signing_key.sign(&canonical).to_bytes()));
        let proof = verify_request_at(request, NOW);
        assert_eq!(
            proof.status,
            AuthorizationReceiptVerificationStatusV1::SignatureInvalid
        );
        assert!(!proof.signature_verified);
        assert!(proof.clock_verified);
        assert!(proof.key_lifecycle_verified);
    }

    #[test]
    fn refuses_expired_or_overlong_receipt_windows() {
        let expired = verify_request_at(fixture_request(NOW - 2_000, NOW), NOW);
        assert_eq!(
            expired.status,
            AuthorizationReceiptVerificationStatusV1::ReceiptClockInvalid
        );
        let overlong = verify_request_at(
            fixture_request(NOW - 1_000, NOW + MAX_AUTHORIZATION_RECEIPT_LIFETIME_MS),
            NOW,
        );
        assert_eq!(
            overlong.status,
            AuthorizationReceiptVerificationStatusV1::ReceiptClockInvalid
        );
    }

    #[test]
    fn refuses_revoked_key_even_when_signature_bytes_match() {
        let mut request = fixture_request(NOW - 1_000, NOW + 1_000);
        request.verification_key.status = IdentityStatus::Revoked;
        request.verification_key.revoked_at = Some(NOW - 1);
        let proof = verify_request_at(request, NOW);
        assert_eq!(
            proof.status,
            AuthorizationReceiptVerificationStatusV1::KeyLifecycleInvalid
        );
        assert!(!proof.key_lifecycle_verified);
        assert!(!proof.signature_verified);
    }

    #[test]
    fn refuses_key_that_was_not_active_at_receipt_authorization_time() {
        let mut request = fixture_request(NOW - 2_000, NOW + 1_000);
        request.verification_key.created_at = NOW;
        request.verification_key.activated_at = NOW;
        let proof = verify_request_at(request, NOW);
        assert_eq!(
            proof.status,
            AuthorizationReceiptVerificationStatusV1::KeyLifecycleInvalid
        );
        assert!(proof.clock_verified);
        assert!(!proof.key_lifecycle_verified);
    }

    #[test]
    fn request_is_closed_and_input_is_bounded() {
        let request = fixture_request(NOW - 1_000, NOW + 1_000);
        let mut value = serde_json::to_value(&request).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("checked_at_ms".to_owned(), serde_json::json!(NOW));
        assert!(
            serde_json::from_value::<AuthorizationReceiptVerificationRequestV1>(value).is_err()
        );

        let oversized = vec![b' '; MAX_VERIFICATION_REQUEST_BYTES + 1];
        assert!(read_request(oversized.as_slice()).is_err());
    }

    #[test]
    fn proof_shape_is_closed_and_stable() {
        let proof = verify_request_at(fixture_request(NOW - 1_000, NOW + 1_000), NOW);
        let object = serde_json::to_value(proof)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            object,
            std::collections::BTreeSet::from([
                "algorithm".to_owned(),
                "checked_at_ms".to_owned(),
                "clock_verified".to_owned(),
                "issuer".to_owned(),
                "key_id".to_owned(),
                "key_lifecycle_verified".to_owned(),
                "receipt_digest".to_owned(),
                "schema".to_owned(),
                "signature_verified".to_owned(),
                "status".to_owned(),
            ])
        );
    }
}
