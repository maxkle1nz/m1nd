//! Cross-version crypto continuity: the vectors that must survive a RustCrypto
//! major bump.
//!
//! Two invariants live here, both of them about *already persisted* bytes:
//!
//! 1. **Hash stability.** `digest_domain_bytes` is the domain-separated SHA-256
//!    that stamps every authority receipt, WAL record and signed-body digest in
//!    the workspace. Its output is pinned to answers computed by an INDEPENDENT
//!    oracle (Python `hashlib`), never re-derived from the code under test, so a
//!    silent change in the hashing stack fails here instead of orphaning stored
//!    digests.
//!
//! 2. **Signature continuity.** Authority-WAL records, authorization receipts
//!    and enclave-provisioned capabilities carry Ed25519 and P-256 signatures
//!    minted by whatever crate version was current when they were written. The
//!    verifier must keep verifying those exact bytes forever.
//!
//! ## Fixture provenance
//!
//! Every signature below was produced on `origin/main` @ 47dbb532 by the
//! PRE-SWEEP stack — `ed25519-dalek 2.2.0`, `p256 0.13.2`, `ecdsa 0.16.9`,
//! `sha2 0.10.9` — via a throwaway generator driving this crate's own public
//! signing API. The inputs are fully deterministic and therefore reproducible:
//! Ed25519 seed `[7u8; 32]`, P-256 secret scalar `[11u8; 32]`, Ed25519 signing
//! is deterministic by construction and RustCrypto ECDSA signs via RFC6979.
//! Nothing here is a live key: both secrets are constant test material.

use std::collections::BTreeMap;

use m1nd_control::{
    digest_canonical, digest_domain_bytes, verify_authority_message_signature, verify_capability,
    ActionId, ActiveMode, AuthorityCapabilityV1, AuthorityCryptoError, AuthorityVariant,
    CapabilityVerificationContext, CryptographicIntegrity, IdentityStatus, OpaqueSignature,
    VerificationKeyRegistryV1, VerificationKeyV1, AUTHORITY_CAPABILITY_SCHEMA,
    ECDSA_P256_SHA256_X962_ALGORITHM, ED25519_ALGORITHM, VERIFICATION_KEY_REGISTRY_SCHEMA,
};

const NOW: u64 = 2_000;
const SKEW: u64 = 100;

/// The exact authority message the fixtures were signed over: the crate's
/// length-delimited envelope around a capability-signature domain and a small
/// JSON payload.
const MESSAGE_HEX: &str = "6d316e642d617574686f726974792d7369676e61747572652d6d6573736167652d76310000000000000000266d316e642d617574686f726974792d6361706162696c6974792d7369676e61747572652d763100000000000000407b22617574686f72697a65645f6174223a313730303030303030303030302c2272656365697074223a22636f6e74696e756974792d766563746f722d7631227d";

const ED25519_PUBLIC_KEY: &str = "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";
const ED25519_SIGNATURE: &str = "307733f4cefba48e1b74fb5c0e1c737dfb0d75b1aa435fec12fb5b7961407813de04019323bc3213c1019ab947957bc8be57d3dfa6d1f7a0fcb11a6d83b39d0b";

const P256_PUBLIC_KEY: &str = "04209c317b637935dd3da1c54f63495dfb31f97d293df085710320595c9aacb83fdde4c69fc17a0c74c20cc692662f049892ba37a4ba47d2c70cd8a99986391f9b";
/// Canonical low-S ASN.1 DER — the only shape the verifier accepts at rest.
const P256_SIGNATURE_LOW_S: &str = "30440220152508d38b7a86dc661f0ea8bd66aa247ec9f0badac335b61e657f73b135bbd10220323f3c8525bc64d54d12215e8966aa1345311000ddfe0c689e30eceeb8c8e944";
/// The same signature in its high-S representation. Cryptographically valid,
/// but non-canonical at rest, so the verifier must refuse it.
const P256_SIGNATURE_HIGH_S: &str = "30450220152508d38b7a86dc661f0ea8bd66aa247ec9f0badac335b61e657f73b135bbd1022100cdc0c379da439b2bb2eddea1769955ec77b5eaacc919921c5588ddd4439a3c0d";

/// Signatures over the full canonical-JSON body of the capability built by
/// [`continuity_capability`], produced through `sign_capability`.
const ED25519_CAPABILITY_SIGNATURE: &str = "97fa653137699b347e9d07b63ec1fa3635d9afb1f9a43d8b540a97a397629061a2b6a173724dc207df6190338e01a274c619b9e0d81420aa4cfb6f7b903be008";
const P256_CAPABILITY_SIGNATURE: &str = "304402201f3c10eae3a2873c172cff1b2f92d3940d102f0f7e46acf2add8489e09fc130d02203cc0ff7c10e497716f9d8c60bf36a87e0e1aa5fcf10f9968e4a2ff21fffa3e3d";

fn decode_hex(value: &str) -> Vec<u8> {
    assert!(value.len().is_multiple_of(2), "hex fixture must be even");
    (0..value.len() / 2)
        .map(|index| {
            u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).expect("hex fixture digit")
        })
        .collect()
}

fn key_record(key_id: &str, algorithm: &str, public_key: &str) -> VerificationKeyV1 {
    VerificationKeyV1 {
        key_id: key_id.to_owned(),
        subject_id: "owner-1".to_owned(),
        algorithm: algorithm.to_owned(),
        public_key: public_key.to_owned(),
        created_at: 1_000,
        activated_at: 1_000,
        expires_at: None,
        revoked_at: None,
        rotated_at: None,
        replacement_key_id: None,
        status: IdentityStatus::Active,
    }
}

fn registry() -> VerificationKeyRegistryV1 {
    VerificationKeyRegistryV1 {
        schema: VERIFICATION_KEY_REGISTRY_SCHEMA.to_owned(),
        registry_epoch: 1,
        keys: BTreeMap::from([
            (
                "owner-ed25519-1".to_owned(),
                key_record("owner-ed25519-1", ED25519_ALGORITHM, ED25519_PUBLIC_KEY),
            ),
            (
                "owner-p256-1".to_owned(),
                key_record(
                    "owner-p256-1",
                    ECDSA_P256_SHA256_X962_ALGORITHM,
                    P256_PUBLIC_KEY,
                ),
            ),
        ]),
    }
}

/// The exact record the persisted capability signatures cover. Any field change
/// here invalidates the fixtures, which is the point: the canonical-JSON body is
/// part of the signed message.
fn continuity_capability(
    algorithm: &str,
    issuer_key_id: &str,
    signature: &str,
) -> AuthorityCapabilityV1 {
    AuthorityCapabilityV1 {
        schema: AUTHORITY_CAPABILITY_SCHEMA.to_owned(),
        capability_id: "cap-continuity-1".to_owned(),
        issuer_subject_id: "owner-1".to_owned(),
        issuer_key_id: issuer_key_id.to_owned(),
        algorithm: algorithm.to_owned(),
        subject_id: "agent-1".to_owned(),
        audience: "m1nd-control".to_owned(),
        organism_id: "organism-1".to_owned(),
        brain_id: "brain-1".to_owned(),
        mission_id: None,
        mission_head_id: None,
        action: ActionId::new("memorize").expect("action id"),
        authority_variant: AuthorityVariant::Human,
        active_mode: ActiveMode::HumanGated,
        payload_digest: "sha256:continuity".to_owned(),
        policy_registry_digest: "sha256:policy".to_owned(),
        constitution_digest: "sha256:constitution".to_owned(),
        key_registry_epoch: 1,
        issued_at: 1_500,
        expires_at: 900_000,
        nonce: "nonce-continuity-1".to_owned(),
        signature: OpaqueSignature::new(signature),
    }
}

fn capability_context() -> CapabilityVerificationContext<'static> {
    CapabilityVerificationContext {
        now_ms: NOW,
        max_future_clock_skew_ms: SKEW,
        expected_schema: AUTHORITY_CAPABILITY_SCHEMA,
        expected_audience: "m1nd-control",
        expected_subject_id: "agent-1",
        expected_payload_digest: "sha256:continuity",
        expected_organism_id: "organism-1",
        expected_brain_id: "brain-1",
        expected_mission_id: None,
        expected_mission_head_id: None,
        expected_action: "memorize",
        expected_authority_variant: AuthorityVariant::Human,
        expected_active_mode: ActiveMode::HumanGated,
    }
}

// ---------------------------------------------------------------------------
// 1. Hash stability
// ---------------------------------------------------------------------------

/// Known-answer vectors for the domain-separated digest, computed independently
/// of this codebase. If the hashing stack ever changes shape, every persisted
/// `signed_body_digest`, receipt digest and WAL head would silently stop
/// matching; this fails first instead.
#[test]
fn domain_separated_digest_matches_pinned_known_answers() {
    for (domain, payload, expected) in [
        (
            "",
            "".as_bytes(),
            "21050b19bf447cbf61bb3931d2e87a7c433e9f171843b4ee6bef973f19291bb4",
        ),
        (
            "abc",
            "abc".as_bytes(),
            "696c178e8f074bb75140921a383962ad6ce86e6722e84e4d48379fe57bdff8aa",
        ),
        (
            "m1nd-authority-signed-body-v1",
            "{}".as_bytes(),
            "385a588a312963c2cc67c54ee7b118584622b56c21cf5bd2929c72bb551c52ce",
        ),
    ] {
        let observed = digest_domain_bytes(domain, payload);
        assert_eq!(
            observed, expected,
            "domain-separated digest for domain '{domain}' changed; persisted digests would be orphaned"
        );
        assert_eq!(observed.len(), 64, "a SHA-256 hex digest is 64 chars");
        assert!(
            observed
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "digest must be lowercase hex"
        );
    }
}

/// The domain separator must actually separate: same payload, different domain,
/// different digest. Guards against a hashing change that drops the prefix.
#[test]
fn domain_separation_is_load_bearing() {
    assert_ne!(
        digest_domain_bytes("a", b"bc"),
        digest_domain_bytes("ab", b"c"),
        "length-delimited framing must make these distinguishable"
    );
    assert_ne!(
        digest_domain_bytes("abc", b"abc"),
        "696c178e8f074bb75140921a383962ad6ce86e6722e84e4d48379fe57bdff8ab",
        "a near-miss vector must not compare equal"
    );
}

/// `digest_canonical` is `digest_domain_bytes` over canonical JSON; pinning it
/// keeps the serializer and the hasher moving together.
#[test]
fn canonical_digest_is_stable_over_the_empty_object() {
    let observed = digest_canonical("m1nd-authority-signed-body-v1", &serde_json::json!({}))
        .expect("canonical digest");
    assert_eq!(
        observed, "385a588a312963c2cc67c54ee7b118584622b56c21cf5bd2929c72bb551c52ce",
        "canonical JSON of {{}} is `{{}}`, so this must equal the pinned domain digest"
    );
}

// ---------------------------------------------------------------------------
// 2. Signature continuity
// ---------------------------------------------------------------------------

/// An Ed25519 signature minted by the pre-sweep stack still verifies through
/// the raw-message seam used by the offline receipt verifier, the enclave
/// authority runtime and the mission-service transport.
#[test]
fn pre_sweep_ed25519_signature_still_verifies() {
    let message = decode_hex(MESSAGE_HEX);
    let key = key_record("owner-ed25519-1", ED25519_ALGORITHM, ED25519_PUBLIC_KEY);
    let integrity = verify_authority_message_signature(
        &message,
        &OpaqueSignature::new(ED25519_SIGNATURE),
        &key,
    )
    .expect("a signature persisted by ed25519-dalek 2.2.0 must still verify");
    assert_eq!(integrity, CryptographicIntegrity::VerifiedEd25519);
}

/// The same, for P-256 ECDSA in canonical low-S DER.
#[test]
fn pre_sweep_p256_signature_still_verifies() {
    let message = decode_hex(MESSAGE_HEX);
    let key = key_record(
        "owner-p256-1",
        ECDSA_P256_SHA256_X962_ALGORITHM,
        P256_PUBLIC_KEY,
    );
    let integrity = verify_authority_message_signature(
        &message,
        &OpaqueSignature::new(P256_SIGNATURE_LOW_S),
        &key,
    )
    .expect("a signature persisted by p256 0.13.2 must still verify");
    assert_eq!(
        integrity,
        CryptographicIntegrity::VerifiedEcdsaP256Sha256X962
    );
}

/// Low-S canonicalization is a *storage* rule, not just a verification detail:
/// a high-S DER encoding of an otherwise valid signature must be refused, so
/// signature bytes at rest have exactly one accepted representation and cannot
/// be malleated into a second valid record.
///
/// This is the vector that pins `ecdsa`'s low-S normalization semantics across
/// the major bump. It fails loudly if normalization ever stops being applied.
#[test]
fn high_s_signature_at_rest_is_refused() {
    let message = decode_hex(MESSAGE_HEX);
    let key = key_record(
        "owner-p256-1",
        ECDSA_P256_SHA256_X962_ALGORITHM,
        P256_PUBLIC_KEY,
    );
    let error = verify_authority_message_signature(
        &message,
        &OpaqueSignature::new(P256_SIGNATURE_HIGH_S),
        &key,
    )
    .expect_err("a high-S DER signature must never be accepted at rest");
    assert!(
        matches!(error, AuthorityCryptoError::SignatureEncoding),
        "high-S must be refused as a non-canonical encoding, got {error:?}"
    );
}

/// A full authority capability signed by the pre-sweep Ed25519 stack still
/// verifies through the record seam — canonical JSON, domain envelope and
/// signature check together.
#[test]
fn pre_sweep_ed25519_capability_still_verifies() {
    let capability = continuity_capability(
        ED25519_ALGORITHM,
        "owner-ed25519-1",
        ED25519_CAPABILITY_SIGNATURE,
    );
    let verified = verify_capability(&capability, &registry(), capability_context())
        .expect("a capability signed before the sweep must still verify");
    assert_eq!(verified.integrity, CryptographicIntegrity::VerifiedEd25519);
    assert_eq!(verified.key_id, "owner-ed25519-1");
}

/// The same, for a capability signed with P-256.
#[test]
fn pre_sweep_p256_capability_still_verifies() {
    let capability = continuity_capability(
        ECDSA_P256_SHA256_X962_ALGORITHM,
        "owner-p256-1",
        P256_CAPABILITY_SIGNATURE,
    );
    let verified = verify_capability(&capability, &registry(), capability_context())
        .expect("a capability signed before the sweep must still verify");
    assert_eq!(
        verified.integrity,
        CryptographicIntegrity::VerifiedEcdsaP256Sha256X962
    );
    assert_eq!(verified.key_id, "owner-p256-1");
}

// ---------------------------------------------------------------------------
// 3. Negative controls — the fixtures above only mean something if these fail
// ---------------------------------------------------------------------------

/// Corrupting a single byte of each persisted signature must break
/// verification. Without this, the vectors above could pass against a verifier
/// that accepted anything.
#[test]
fn corrupting_a_fixture_signature_breaks_verification() {
    let message = decode_hex(MESSAGE_HEX);
    for (algorithm, public_key, key_id, signature) in [
        (
            ED25519_ALGORITHM,
            ED25519_PUBLIC_KEY,
            "owner-ed25519-1",
            ED25519_SIGNATURE,
        ),
        (
            ECDSA_P256_SHA256_X962_ALGORITHM,
            P256_PUBLIC_KEY,
            "owner-p256-1",
            P256_SIGNATURE_LOW_S,
        ),
    ] {
        let mut bytes = decode_hex(signature);
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        let corrupted: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let key = key_record(key_id, algorithm, public_key);
        verify_authority_message_signature(&message, &OpaqueSignature::new(&corrupted), &key)
            .expect_err("a one-bit signature corruption must be rejected");
    }
}

/// Corrupting the signed message must break verification too — proving the
/// fixtures bind to these exact bytes and not to some looser property.
#[test]
fn corrupting_the_signed_message_breaks_verification() {
    let mut message = decode_hex(MESSAGE_HEX);
    let last = message.len() - 1;
    message[last] ^= 0x01;
    for (algorithm, public_key, key_id, signature) in [
        (
            ED25519_ALGORITHM,
            ED25519_PUBLIC_KEY,
            "owner-ed25519-1",
            ED25519_SIGNATURE,
        ),
        (
            ECDSA_P256_SHA256_X962_ALGORITHM,
            P256_PUBLIC_KEY,
            "owner-p256-1",
            P256_SIGNATURE_LOW_S,
        ),
    ] {
        let key = key_record(key_id, algorithm, public_key);
        verify_authority_message_signature(&message, &OpaqueSignature::new(signature), &key)
            .expect_err("a one-bit message corruption must be rejected");
    }
}

/// A capability body edited after signing must fail, even though the signature
/// bytes themselves are untouched.
#[test]
fn editing_a_signed_capability_body_breaks_verification() {
    let mut capability = continuity_capability(
        ED25519_ALGORITHM,
        "owner-ed25519-1",
        ED25519_CAPABILITY_SIGNATURE,
    );
    capability.payload_digest = "sha256:tampered".to_owned();
    let mut context = capability_context();
    context.expected_payload_digest = "sha256:tampered";
    let error = verify_capability(&capability, &registry(), context)
        .expect_err("a tampered signed body must be rejected");
    assert!(
        matches!(error, AuthorityCryptoError::SignatureInvalid),
        "tampering must fail the signature check, got {error:?}"
    );
}

/// Cross-key verification must fail: the Ed25519 fixture must not verify under
/// the P-256 key record and vice versa.
#[test]
fn fixtures_do_not_verify_under_the_wrong_key() {
    let message = decode_hex(MESSAGE_HEX);
    let ed_key = key_record("owner-ed25519-1", ED25519_ALGORITHM, ED25519_PUBLIC_KEY);
    let p256_key = key_record(
        "owner-p256-1",
        ECDSA_P256_SHA256_X962_ALGORITHM,
        P256_PUBLIC_KEY,
    );
    verify_authority_message_signature(
        &message,
        &OpaqueSignature::new(P256_SIGNATURE_LOW_S),
        &ed_key,
    )
    .expect_err("a P-256 signature must not verify as Ed25519");
    verify_authority_message_signature(
        &message,
        &OpaqueSignature::new(ED25519_SIGNATURE),
        &p256_key,
    )
    .expect_err("an Ed25519 signature must not verify as P-256");
}
