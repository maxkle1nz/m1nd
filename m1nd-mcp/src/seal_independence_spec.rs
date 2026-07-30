//! Seal the owner's `IndependenceSpecV1` — one bounded, offline document step.
//!
//! `IndependenceSpecV1::seal` writes a spec's `independence_spec_digest` from the
//! digest of its own `core`. Until this module, that call had no surface: the
//! owner authors the constitution's four voting seats by hand, but the ceremony
//! (`custody_ceremony::require_a_usable_independence_spec`) refuses any spec whose
//! declared digest is not the digest of its core — so an owner-authored spec was
//! unusable, and the only thing that could seal it was a Rust test.
//!
//! This module is that surface and nothing more. It is in the one-shot CLI family
//! `--verify-authorization-receipt`, `--inbox-sweep`, `--medulla-migrate` and
//! `--custody-ceremony` already established: parse, do ONE bounded offline thing,
//! print one JSON object, exit. It never boots an owner, opens a port, or takes a
//! lease.
//!
//! # What this touches, and what it must never touch
//!
//! It reads one file and computes one digest. It does not open the Secure Enclave,
//! the keychain, the protected root, or any ceremony state — sealing a document is
//! not a ceremony step, and `G9-CUSTODY-CEREMONY.md` §0's prohibition on an agent
//! performing, simulating or dry-running a ceremony step is untouched by it. That
//! is also why this module is NOT `#[cfg(target_os = "macos")]`: there is no
//! platform floor under it to be absent.
//!
//! # The output IS the artifact
//!
//! On success it prints the SEALED SPEC verbatim — a closed, self-describing JSON
//! object that already carries its own `schema` — so the owner's next command can
//! consume it directly:
//!
//! ```text
//! m1nd-mcp --seal-independence-spec draft.json > independence-spec.json
//! m1nd-mcp --custody-ceremony provision-seats \
//!     --custody-protected-root <root> --custody-independence-spec independence-spec.json
//! ```
//!
//! Wrapping it in a receipt envelope would have made that a two-step extraction for
//! no gain. Refusals DO carry an envelope ([`SEAL_INDEPENDENCE_SPEC_REFUSAL_SCHEMA`]),
//! because a refusal is a statement about the attempt rather than a document.
//!
//! The printed key order is `serde_json`'s and is deterministic run to run, but it
//! is NOT the order the owner wrote — and it does not need to be. The digest is
//! computed over the canonical form of the core, so re-sealing a sealed spec
//! returns the same digest, and the ceremony parses the document by name.
//!
//! # Tolerant input, by construction
//!
//! The incoming `independence_spec_digest` is READ AND IGNORED: sealing is the act
//! that decides it, so an empty string, a placeholder, and a stale digest from an
//! earlier draft all behave identically — overwritten. The key must still be
//! present, because the type is `IndependenceSpecV1` itself and its
//! `deny_unknown_fields` is what keeps a misspelled seat field a refusal instead of
//! a silently dropped one. Trading that away for an optional digest would have
//! meant editing `m1nd-control` to loosen the very contract being sealed.
//!
//! # The floors, and where each one comes from
//!
//! Only the structural floors that need no owner-held [`SafetyKernelV1`] run here.
//! Every one mirrors `IndependenceSpecV1::validate_against_kernel`, reading the
//! immutable constant that `SafetyKernelV1::validate` pins the kernel's own field
//! to — so this surface can never disagree with the kernel the ceremony will check
//! the spec against, and no number is written down twice:
//!
//! | floor | anchor in `m1nd-control/src/autonomy.rs` |
//! |---|---|
//! | schema string | `INDEPENDENCE_SPEC_SCHEMA` |
//! | exactly four voting seats | `IMMUTABLE_VERIFIER_SEATS` (kernel field pinned to it) |
//! | quorum at or above the floor, never above the seat count | `IMMUTABLE_QUORUM_THRESHOLD` |
//! | the spec's own domain minimum not lowered | `IMMUTABLE_FAILURE_DOMAINS` |
//! | the seats span that minimum in DISTINCT domains | the spec's `minimum_failure_domains` |
//! | proposer/executor and sentinel non-voting | `validate_against_kernel`'s non-voting rule |
//!
//! What is deliberately NOT checked here is everything that needs the kernel, the
//! constitution, or cryptography: seat principal/key/context uniqueness, the
//! sentinel's exclusion from the voting seats, and the blind-isolation policy
//! digest. `validate_against_kernel` owns those and still runs at the ceremony. A
//! spec this tool seals is well-formed enough to be presented; it is not thereby
//! ratified.
//!
//! [`SafetyKernelV1`]: m1nd_control::autonomy::SafetyKernelV1

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use m1nd_control::autonomy::{
    IndependenceSpecV1, IMMUTABLE_FAILURE_DOMAINS, IMMUTABLE_QUORUM_THRESHOLD,
    IMMUTABLE_VERIFIER_SEATS, INDEPENDENCE_SPEC_SCHEMA,
};

/// Schema of the refusal envelope this mode prints. Its own, distinct from the
/// custody ceremony's: sealing a document is not a ceremony step, and nothing
/// downstream may read one refusal as the other.
pub const SEAL_INDEPENDENCE_SPEC_REFUSAL_SCHEMA: &str = "m1nd-seal-independence-spec-refusal-v1";

/// Why a seal refused. Every variant names one cause with the remedy implied; none
/// of them is an opaque code, and none of them is a warning the owner could seal
/// past — a spec that reaches [`IndependenceSpecV1::seal`] here has cleared all of
/// them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SealRefusalV1 {
    /// The path does not exist, is a directory, or cannot be read.
    Unreadable(String),
    /// The bytes are not JSON, or not this contract — including an unknown field,
    /// which `IndependenceSpecV1`'s `deny_unknown_fields` refuses rather than drop.
    Malformed(String),
    /// The document declares some other contract's schema.
    WrongSchema(String),
    /// The four voting seats are frozen (`IMMUTABLE_VERIFIER_SEATS`).
    SeatCount(usize),
    /// The quorum is below the frozen three-of-four (`IMMUTABLE_QUORUM_THRESHOLD`).
    QuorumBelowFloor(u16),
    /// The quorum exceeds the seats that could ever vote — unreachable, not
    /// stricter.
    QuorumAboveSeatCount { quorum: u16, seats: usize },
    /// The spec declares a domain minimum below the frozen one
    /// (`IMMUTABLE_FAILURE_DOMAINS`).
    DomainMinimumLowered(u16),
    /// The seats do not actually span the minimum the spec declares. Declaring the
    /// floor is not meeting it.
    InsufficientFailureDomains { spanned: usize, required: u16 },
    /// A role that must remain non-voting is marked voting.
    VotingNonvotingRole(&'static str),
    /// The core could not be canonicalized, so no digest exists to seal it with.
    NotCanonical(String),
}

impl SealRefusalV1 {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unreadable(_) => "seal_independence_spec_unreadable",
            Self::Malformed(_) => "seal_independence_spec_malformed",
            Self::WrongSchema(_) => "seal_independence_spec_wrong_schema",
            Self::SeatCount(_) => "seal_independence_spec_seat_count",
            Self::QuorumBelowFloor(_) => "seal_independence_spec_quorum_below_floor",
            Self::QuorumAboveSeatCount { .. } => "seal_independence_spec_quorum_above_seat_count",
            Self::DomainMinimumLowered(_) => "seal_independence_spec_domain_minimum_lowered",
            Self::InsufficientFailureDomains { .. } => {
                "seal_independence_spec_insufficient_failure_domains"
            }
            Self::VotingNonvotingRole(_) => "seal_independence_spec_voting_nonvoting_role",
            Self::NotCanonical(_) => "seal_independence_spec_not_canonical",
        }
    }

    /// The refusal as the one closed JSON object this mode prints. It carries no
    /// `core` and no digest: a refusal is a statement about the attempt, never a
    /// document anyone could mistake for a sealed spec.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": SEAL_INDEPENDENCE_SPEC_REFUSAL_SCHEMA,
            "status": "REFUSED",
            "code": self.code(),
            "detail": self.to_string(),
        })
    }
}

impl fmt::Display for SealRefusalV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(detail) => write!(formatter, "independence spec unreadable: {detail}"),
            Self::Malformed(detail) => write!(
                formatter,
                "the file is not an IndependenceSpecV1: {detail} — every field of the contract is \
                 required and no extra field is accepted, so a misspelled key refuses here rather \
                 than being silently dropped"
            ),
            Self::WrongSchema(actual) => write!(
                formatter,
                "the document declares schema '{actual}', expected '{INDEPENDENCE_SPEC_SCHEMA}'"
            ),
            Self::SeatCount(seats) => write!(
                formatter,
                "the spec names {seats} voting seats; the constitution's voting seats are frozen \
                 at {IMMUTABLE_VERIFIER_SEATS}"
            ),
            Self::QuorumBelowFloor(quorum) => write!(
                formatter,
                "quorum_threshold is {quorum}; the kernel floor is \
                 {IMMUTABLE_QUORUM_THRESHOLD}-of-{IMMUTABLE_VERIFIER_SEATS} and a constitution \
                 cannot reduce it"
            ),
            Self::QuorumAboveSeatCount { quorum, seats } => write!(
                formatter,
                "quorum_threshold is {quorum} over {seats} voting seats; a quorum no seat count \
                 can reach is unreachable, not stricter"
            ),
            Self::DomainMinimumLowered(minimum) => write!(
                formatter,
                "minimum_failure_domains is {minimum}; the kernel floor is \
                 {IMMUTABLE_FAILURE_DOMAINS} and a constitution cannot reduce failure-domain \
                 diversity"
            ),
            Self::InsufficientFailureDomains { spanned, required } => write!(
                formatter,
                "the voting seats span {spanned} distinct failure domains but the spec requires \
                 {required}; declaring the minimum is not meeting it"
            ),
            Self::VotingNonvotingRole(field) => write!(
                formatter,
                "core.{field} is false; proposer, executor and sentinel remain non-voting"
            ),
            Self::NotCanonical(detail) => write!(
                formatter,
                "the spec core could not be canonicalized, so there is no digest to seal it with: \
                 {detail}"
            ),
        }
    }
}

impl std::error::Error for SealRefusalV1 {}

/// The structural floors that need no owner-held `SafetyKernelV1`.
///
/// Each one mirrors a rule of `IndependenceSpecV1::validate_against_kernel`, with
/// the kernel field that rule reads replaced by the immutable constant
/// `SafetyKernelV1::validate` pins that field to. Two consequences worth stating:
/// this surface cannot drift from the kernel the ceremony will check the spec
/// against, and no floor is written down as a number here.
///
/// Order is deliberate — the coarsest fact first — so the refusal an owner sees
/// names the outermost thing that is wrong rather than a consequence of it.
fn require_structural_floors(spec: &IndependenceSpecV1) -> Result<(), SealRefusalV1> {
    if spec.schema != INDEPENDENCE_SPEC_SCHEMA {
        return Err(SealRefusalV1::WrongSchema(spec.schema.clone()));
    }

    let seats = spec.core.voting_verifiers.len();
    if seats != usize::from(IMMUTABLE_VERIFIER_SEATS) {
        return Err(SealRefusalV1::SeatCount(seats));
    }

    if spec.core.quorum_threshold < IMMUTABLE_QUORUM_THRESHOLD {
        return Err(SealRefusalV1::QuorumBelowFloor(spec.core.quorum_threshold));
    }
    if usize::from(spec.core.quorum_threshold) > seats {
        return Err(SealRefusalV1::QuorumAboveSeatCount {
            quorum: spec.core.quorum_threshold,
            seats,
        });
    }

    if spec.core.minimum_failure_domains < IMMUTABLE_FAILURE_DOMAINS {
        return Err(SealRefusalV1::DomainMinimumLowered(
            spec.core.minimum_failure_domains,
        ));
    }
    let spanned: BTreeSet<&str> = spec
        .core
        .voting_verifiers
        .iter()
        .map(|seat| seat.failure_domain.as_str())
        .collect();
    if spanned.len() < usize::from(spec.core.minimum_failure_domains) {
        return Err(SealRefusalV1::InsufficientFailureDomains {
            spanned: spanned.len(),
            required: spec.core.minimum_failure_domains,
        });
    }

    if !spec.core.proposer_executor_nonvoting {
        return Err(SealRefusalV1::VotingNonvotingRole(
            "proposer_executor_nonvoting",
        ));
    }
    if !spec.core.sentinel_nonvoting {
        return Err(SealRefusalV1::VotingNonvotingRole("sentinel_nonvoting"));
    }

    Ok(())
}

/// Read the spec at `path`, clear the structural floors, and seal it.
///
/// The incoming `independence_spec_digest` is never inspected: sealing is the act
/// that decides it. Nothing else in the document is touched — the sealed spec's
/// `core` is the core that was read.
pub fn seal_independence_spec_at(path: &Path) -> Result<IndependenceSpecV1, SealRefusalV1> {
    let bytes = std::fs::read(path)
        .map_err(|error| SealRefusalV1::Unreadable(format!("{}: {error}", path.display())))?;
    let mut spec: IndependenceSpecV1 = serde_json::from_slice(&bytes)
        .map_err(|error| SealRefusalV1::Malformed(format!("{}: {error}", path.display())))?;
    require_structural_floors(&spec)?;
    spec.seal()
        .map_err(|error| SealRefusalV1::NotCanonical(error.to_string()))?;
    Ok(spec)
}

/// Read the spec at `path`, run the structural floors, seal it, and return the
/// JSON to print with the process exit code.
///
/// `(sealed spec, 0)` on success; `(refusal envelope, 1)` on every refusal.
pub fn run_seal_independence_spec(path: &Path) -> (serde_json::Value, i32) {
    match seal_independence_spec_at(path) {
        Ok(sealed) => match serde_json::to_value(&sealed) {
            Ok(document) => (document, 0),
            Err(error) => (SealRefusalV1::NotCanonical(error.to_string()).to_json(), 1),
        },
        Err(refusal) => (refusal.to_json(), 1),
    }
}

// ===========================================================================
// The battery.
//
// It runs on every target — there is no platform floor under a document seal —
// and writes only into a temp directory. It never constructs, reads or names a
// ceremony artifact.
// ===========================================================================
#[cfg(test)]
mod tests {
    use m1nd_control::autonomy::{
        IndependenceSpecCoreV1, IndependenceSpecV1, VerifierSeatV1, IMMUTABLE_FAILURE_DOMAINS,
        IMMUTABLE_QUORUM_THRESHOLD, IMMUTABLE_VERIFIER_SEATS, INDEPENDENCE_SPEC_SCHEMA,
    };
    use tempfile::TempDir;

    use super::*;

    /// The shape the owner authored: four voting seats over four distinct failure
    /// domains, quorum at the frozen three-of-four, both non-voting booleans true,
    /// and an EMPTY digest — the field this tool exists to fill.
    ///
    /// Mirrors the fixture shape in `custody_ceremony.rs`'s battery, with its own
    /// values: two batteries sharing one set of principals would make a lineage
    /// collision invisible.
    fn owner_authored_spec() -> IndependenceSpecV1 {
        let voting_verifiers = [
            ("principal-north", "seat-north", "domain-north", "1"),
            ("principal-south", "seat-south", "domain-south", "2"),
            ("principal-east", "seat-east", "domain-east", "3"),
            ("principal-west", "seat-west", "domain-west", "4"),
        ]
        .into_iter()
        .map(
            |(principal_id, key_id, failure_domain, context)| VerifierSeatV1 {
                principal_id: principal_id.to_owned(),
                key_id: key_id.to_owned(),
                failure_domain: failure_domain.to_owned(),
                parent_session_context_digest: context.repeat(64),
            },
        )
        .collect();
        IndependenceSpecV1 {
            schema: INDEPENDENCE_SPEC_SCHEMA.to_owned(),
            core: IndependenceSpecCoreV1 {
                constitution_epoch: 1,
                voting_verifiers,
                quorum_threshold: IMMUTABLE_QUORUM_THRESHOLD,
                minimum_failure_domains: IMMUTABLE_FAILURE_DOMAINS,
                blind_isolation_policy_digest: "9".repeat(64),
                nonvoting_sentinel_id: "sentinel-north".to_owned(),
                proposer_executor_nonvoting: true,
                sentinel_nonvoting: true,
            },
            independence_spec_digest: String::new(),
        }
    }

    /// Write `value` as the spec file and seal it, returning the JSON and exit code.
    fn seal_json(temp: &TempDir, value: &serde_json::Value) -> (serde_json::Value, i32) {
        let path = temp.path().join("independence-spec.json");
        std::fs::write(&path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
        run_seal_independence_spec(&path)
    }

    fn seal_spec(temp: &TempDir, spec: &IndependenceSpecV1) -> (serde_json::Value, i32) {
        seal_json(temp, &serde_json::to_value(spec).unwrap())
    }

    /// Every refusal is exit 1 and a NAMED code in the refusal envelope.
    fn refusal_code(payload: &serde_json::Value, code: i32) -> String {
        assert_eq!(code, 1, "a refusal always exits 1: {payload}");
        assert_eq!(
            payload["schema"], SEAL_INDEPENDENCE_SPEC_REFUSAL_SCHEMA,
            "a refusal carries its own envelope"
        );
        assert_eq!(payload["status"], "REFUSED");
        assert!(
            payload["detail"].as_str().is_some_and(|d| !d.is_empty()),
            "a refusal always says why: {payload}"
        );
        payload["code"]
            .as_str()
            .unwrap_or_else(|| panic!("a refusal always names itself: {payload}"))
            .to_owned()
    }

    // -----------------------------------------------------------------------
    // 1. The seal itself
    // -----------------------------------------------------------------------

    /// The acceptance case: the owner's authored shape seals, exit 0, and what is
    /// printed is the spec — a document his next command can consume directly.
    #[test]
    fn the_owner_authored_shape_seals() {
        let temp = TempDir::new().unwrap();
        let (payload, code) = seal_spec(&temp, &owner_authored_spec());

        assert_eq!(code, 0, "the authored shape seals: {payload}");
        let sealed: IndependenceSpecV1 = serde_json::from_value(payload.clone())
            .expect("what is printed round-trips as an IndependenceSpecV1");
        assert_eq!(sealed.schema, INDEPENDENCE_SPEC_SCHEMA);
        assert_eq!(
            sealed.core.voting_verifiers.len(),
            usize::from(IMMUTABLE_VERIFIER_SEATS)
        );
    }

    /// The round trip: the printed digest is the in-Rust digest of the same core,
    /// computed independently here. This is the whole product — a spec sealed by
    /// this tool is a spec the ceremony's own digest check will accept.
    #[test]
    fn the_printed_digest_is_the_digest_of_the_printed_core() {
        let temp = TempDir::new().unwrap();
        let (payload, code) = seal_spec(&temp, &owner_authored_spec());
        assert_eq!(code, 0, "{payload}");

        let sealed: IndependenceSpecV1 = serde_json::from_value(payload).unwrap();
        assert_eq!(
            sealed.independence_spec_digest,
            sealed.compute_digest().expect("the core is canonical"),
            "the sealed digest is the digest of the core it was sealed over"
        );

        let mut independently = owner_authored_spec();
        independently.seal().expect("the fixture seals in Rust");
        assert_eq!(
            sealed.independence_spec_digest, independently.independence_spec_digest,
            "the CLI and the library seal the same core to the same digest"
        );
    }

    /// The pin that makes this tool safe to run on a real constitution: sealing
    /// writes ONE field. The core that comes out is byte-identical to the core that
    /// went in — no reordering, no normalisation, no quiet repair.
    #[test]
    fn sealing_never_alters_the_core() {
        let temp = TempDir::new().unwrap();
        let authored = serde_json::to_value(owner_authored_spec()).unwrap();
        let (payload, code) = seal_json(&temp, &authored);
        assert_eq!(code, 0, "{payload}");

        assert_eq!(
            serde_json::to_string(&authored["core"]).unwrap(),
            serde_json::to_string(&payload["core"]).unwrap(),
            "the core is carried through byte for byte"
        );
        assert_eq!(
            authored["schema"], payload["schema"],
            "the schema is carried through untouched"
        );
    }

    /// Tolerant input, the reason this tool exists: the incoming digest is whatever
    /// the owner left there. Empty, placeholder, or stale from an earlier draft —
    /// all three seal to the same digest of the same core, none of them refuses.
    #[test]
    fn any_incoming_digest_is_overwritten_rather_than_refused() {
        let temp = TempDir::new().unwrap();
        let mut expected: Option<String> = None;

        for incoming in ["", "PLACEHOLDER", &"f".repeat(64), &"0".repeat(64)] {
            let mut spec = owner_authored_spec();
            spec.independence_spec_digest = incoming.to_owned();
            let (payload, code) = seal_spec(&temp, &spec);
            assert_eq!(code, 0, "incoming digest {incoming:?} seals: {payload}");

            let sealed: IndependenceSpecV1 = serde_json::from_value(payload).unwrap();
            assert_ne!(
                sealed.independence_spec_digest, incoming,
                "the incoming digest is never the sealed one unless it was already right"
            );
            match &expected {
                None => expected = Some(sealed.independence_spec_digest),
                Some(first) => assert_eq!(
                    &sealed.independence_spec_digest, first,
                    "the seal depends on the core alone, never on what was in the digest field"
                ),
            }
        }
    }

    // -----------------------------------------------------------------------
    // 2. The named refusals
    // -----------------------------------------------------------------------

    /// A path that is not there, or not readable, refuses by name — it never
    /// invents an empty spec to seal.
    #[test]
    fn an_unreadable_path_refuses_by_name() {
        let temp = TempDir::new().unwrap();
        let (payload, code) = run_seal_independence_spec(&temp.path().join("absent.json"));
        assert_eq!(
            refusal_code(&payload, code),
            "seal_independence_spec_unreadable"
        );
    }

    /// Bytes that are not JSON refuse by name.
    #[test]
    fn bytes_that_are_not_json_refuse_by_name() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("independence-spec.json");
        std::fs::write(&path, b"{ this is not json").unwrap();
        let (payload, code) = run_seal_independence_spec(&path);
        assert_eq!(
            refusal_code(&payload, code),
            "seal_independence_spec_malformed"
        );
    }

    /// JSON that is not this contract refuses by name — including a spec whose
    /// seats carry an unknown field, which `deny_unknown_fields` catches. Keeping
    /// the strict type is exactly what makes a misspelled seat key a refusal
    /// instead of a silently dropped seat.
    #[test]
    fn json_that_is_not_the_contract_refuses_by_name() {
        let temp = TempDir::new().unwrap();

        for (label, value) in [
            ("empty object", serde_json::json!({})),
            ("unknown top-level field", {
                let mut value = serde_json::to_value(owner_authored_spec()).unwrap();
                value["notes"] = serde_json::json!("hand-written");
                value
            }),
            ("unknown seat field", {
                let mut value = serde_json::to_value(owner_authored_spec()).unwrap();
                value["core"]["voting_verifiers"][0]["seat_number"] = serde_json::json!(1);
                value
            }),
            ("missing digest field", {
                let mut value = serde_json::to_value(owner_authored_spec()).unwrap();
                value
                    .as_object_mut()
                    .unwrap()
                    .remove("independence_spec_digest");
                value
            }),
        ] {
            let (payload, code) = seal_json(&temp, &value);
            assert_eq!(
                refusal_code(&payload, code),
                "seal_independence_spec_malformed",
                "{label} refuses as malformed"
            );
        }
    }

    /// A spec that is not an independence spec refuses by name rather than being
    /// sealed into something that looks like one.
    #[test]
    fn a_wrong_schema_refuses_by_name() {
        let temp = TempDir::new().unwrap();
        let mut spec = owner_authored_spec();
        spec.schema = "m1nd-constitution-store-v1".to_owned();
        let (payload, code) = seal_spec(&temp, &spec);
        assert_eq!(
            refusal_code(&payload, code),
            "seal_independence_spec_wrong_schema"
        );
    }

    /// The four voting seats are frozen (`IMMUTABLE_VERIFIER_SEATS`): neither three
    /// nor five is a constitution this tool will seal.
    #[test]
    fn a_seat_count_other_than_the_frozen_four_refuses_by_name() {
        let temp = TempDir::new().unwrap();

        let mut too_few = owner_authored_spec();
        too_few.core.voting_verifiers.pop();
        let (payload, code) = seal_spec(&temp, &too_few);
        assert_eq!(
            refusal_code(&payload, code),
            "seal_independence_spec_seat_count",
            "three seats refuse"
        );

        let mut too_many = owner_authored_spec();
        let extra = VerifierSeatV1 {
            principal_id: "principal-zenith".to_owned(),
            key_id: "seat-zenith".to_owned(),
            failure_domain: "domain-zenith".to_owned(),
            parent_session_context_digest: "5".repeat(64),
        };
        too_many.core.voting_verifiers.push(extra);
        let (payload, code) = seal_spec(&temp, &too_many);
        assert_eq!(
            refusal_code(&payload, code),
            "seal_independence_spec_seat_count",
            "five seats refuse"
        );
    }

    /// The three-of-four quorum cannot be reduced (`IMMUTABLE_QUORUM_THRESHOLD`).
    #[test]
    fn a_quorum_below_the_kernel_floor_refuses_by_name() {
        let temp = TempDir::new().unwrap();
        let mut spec = owner_authored_spec();
        spec.core.quorum_threshold = IMMUTABLE_QUORUM_THRESHOLD - 1;
        let (payload, code) = seal_spec(&temp, &spec);
        assert_eq!(
            refusal_code(&payload, code),
            "seal_independence_spec_quorum_below_floor"
        );
    }

    /// A quorum larger than the seats that could ever vote is unreachable, not
    /// stricter — `validate_against_kernel` refuses it and so does this.
    #[test]
    fn a_quorum_above_the_seat_count_refuses_by_name() {
        let temp = TempDir::new().unwrap();
        let mut spec = owner_authored_spec();
        spec.core.quorum_threshold = IMMUTABLE_VERIFIER_SEATS + 1;
        let (payload, code) = seal_spec(&temp, &spec);
        assert_eq!(
            refusal_code(&payload, code),
            "seal_independence_spec_quorum_above_seat_count"
        );
    }

    /// The spec may not declare a domain minimum below the frozen one
    /// (`IMMUTABLE_FAILURE_DOMAINS`).
    #[test]
    fn a_lowered_failure_domain_minimum_refuses_by_name() {
        let temp = TempDir::new().unwrap();
        let mut spec = owner_authored_spec();
        spec.core.minimum_failure_domains = IMMUTABLE_FAILURE_DOMAINS - 1;
        let (payload, code) = seal_spec(&temp, &spec);
        assert_eq!(
            refusal_code(&payload, code),
            "seal_independence_spec_domain_minimum_lowered"
        );
    }

    /// Declaring the minimum is not meeting it: four seats crowded into two
    /// domains are not three independent failure domains.
    #[test]
    fn seats_spanning_too_few_domains_refuse_by_name() {
        let temp = TempDir::new().unwrap();
        let mut spec = owner_authored_spec();
        spec.core.voting_verifiers[2].failure_domain = "domain-north".to_owned();
        spec.core.voting_verifiers[3].failure_domain = "domain-south".to_owned();
        let (payload, code) = seal_spec(&temp, &spec);
        assert_eq!(
            refusal_code(&payload, code),
            "seal_independence_spec_insufficient_failure_domains"
        );

        // Exactly three distinct domains over four seats is the floor, not a
        // violation of it — the boundary is inclusive.
        let mut at_the_floor = owner_authored_spec();
        at_the_floor.core.voting_verifiers[3].failure_domain = "domain-north".to_owned();
        let (payload, code) = seal_spec(&temp, &at_the_floor);
        assert_eq!(code, 0, "three distinct domains meets the floor: {payload}");
    }

    /// Proposer, executor and sentinel stay non-voting. Either boolean turned off
    /// is its own refusal.
    #[test]
    fn non_voting_flags_turned_off_refuse_by_name() {
        let temp = TempDir::new().unwrap();

        let mut proposer_votes = owner_authored_spec();
        proposer_votes.core.proposer_executor_nonvoting = false;
        let (payload, code) = seal_spec(&temp, &proposer_votes);
        assert_eq!(
            refusal_code(&payload, code),
            "seal_independence_spec_voting_nonvoting_role",
            "a voting proposer/executor refuses"
        );

        let mut sentinel_votes = owner_authored_spec();
        sentinel_votes.core.sentinel_nonvoting = false;
        let (payload, code) = seal_spec(&temp, &sentinel_votes);
        assert_eq!(
            refusal_code(&payload, code),
            "seal_independence_spec_voting_nonvoting_role",
            "a voting sentinel refuses"
        );
    }

    /// Nothing is printed on a refusal that could be mistaken for a sealed spec:
    /// the envelope carries no `core` and no digest to copy out.
    #[test]
    fn a_refusal_never_prints_a_sealable_document() {
        let temp = TempDir::new().unwrap();
        let mut spec = owner_authored_spec();
        spec.core.quorum_threshold = 0;
        let (payload, code) = seal_spec(&temp, &spec);
        assert_eq!(code, 1);
        assert!(
            payload.get("core").is_none() && payload.get("independence_spec_digest").is_none(),
            "a refusal is a statement about the attempt, never a document: {payload}"
        );
    }
}
