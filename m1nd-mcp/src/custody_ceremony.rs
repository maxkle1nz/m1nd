//! The DOOR to the G9 Secure Enclave custody floor — amendment G9-A1, Path B.
//!
//! The floor itself (`enclave_authority`) was implemented, tested and merged, and
//! `docs/benchmarks/G9-CUSTODY-CEREMONY.md` §4 measured what was missing: every one
//! of its ten public entry points had zero references outside its own file, and
//! `assemble_production_owner_authority_v1` had three callers, all inside
//! `#[cfg(test)]`. `docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md` §7 named this
//! wiring as the next mechanical step. This module is that wiring, and nothing
//! more: it builds the door, not a second floor.
//!
//! # The ingress IS the human-origin fact
//!
//! The surface is a one-shot CLI mode — `--custody-ceremony <verb>` — in the family
//! `--verify-authorization-receipt`, `--inbox-sweep` and `--medulla-migrate` already
//! established: parse, do one bounded thing offline, print one closed JSON object,
//! exit. It never boots an owner, opens a port, or takes a lease.
//!
//! It follows the `--birth` precedent exactly (`brain_birth`, SPEC-2 §2): the
//! ceremony is admitted by a stamp the OWNER constructs about ITSELF from its own
//! ingress, never by data that arrives. [`OwnerCeremonyIngressV1`] has no
//! `Deserialize`, no `FromStr`, no public field and one construction site, and the
//! ceremony entry point takes it BY VALUE — so no MCP tool, REST route, header or
//! payload field can manufacture one. The closed allowlist is the type system.
//!
//! The honest limit, stated where the code is (the same limit `brain_birth`
//! states): a same-UID process can run this command too. This closes the REFLEX
//! vector — the agent holding the MCP surface cannot start a custody ceremony by
//! habit, misconfiguration or a dressed-up payload. It is not a defence against a
//! hostile local process.
//!
//! # What is NOT_RUN here, and is not fakeable
//!
//! `G9-CUSTODY-CEREMONY.md` §0 prohibits an agent from performing, simulating,
//! stubbing or dry-running any ceremony step. Every verb below now REACHES the
//! floor rather than answering from a placeholder — and that is the point at
//! which each step's real prerequisite starts refusing. What the code can do is
//! ask the platform honestly and report what it said; what it cannot do is stand
//! in for the platform. These remain NOT_RUN until the owner runs them on an
//! Apple Silicon / T2 Mac, present, with Touch ID enrolled, on a codesigned
//! binary carrying the `KeychainAccessGroups` entitlement:
//!
//! * Phase A steps 1-2 — the four verifier seats as REAL Secure Enclave keys,
//!   persisted in the data-protection keychain. An unentitled binary cannot
//!   persist or resolve one, so on any other build this refuses naming P4.
//! * Phase A step 3 — the `kSecAccessControl` conformance check. The flag values
//!   are hand-rolled and `SecKeyCopyAttributes` does not read access control back,
//!   so the owner's live run is the only thing that proves them (§5 R5). Nothing
//!   in this module claims otherwise, and nothing here can.
//! * Phase B step 4 — the owner's biometric seat. Minting it is not using it: the
//!   Touch ID prompt is raised by the key's own access control when it SIGNS, and
//!   that hardware gate has no stand-in.
//! * Phase C steps 5-7 — open, re-attest and seal against real enclave keys.
//! * Phase C step 8 — retiring the live proof key, an owner operational step.
//!
//! There is deliberately no simulation path, and the battery asserts one is never
//! added (`m1nd-mcp/tests/custody_ceremony_wiring.rs`).
//!
//! # The ceremony invents nothing it could be asked for
//!
//! Every identity the ceremony seals comes from an artifact the OWNER presents:
//! the four seats' principals, key ids and failure domains are read out of the
//! owner's `IndependenceSpecV1`, and the constitution digest is passed in. A
//! ceremony that fabricated its own seats would make `bind_independence_spec` a
//! tautology — it would be binding to what it had just made up — so the paths are
//! owner-held here for the same reason the protected root is.

use std::fmt;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// The ceremony's step list as CLI verbs, in the order the owner runs them
/// (`G9-CUSTODY-CEREMONY.md` §2). A closed set: an unrecognised verb refuses
/// rather than resolving to a step, and there is no default.
pub const CUSTODY_CEREMONY_VERBS: &[&str] = &[
    "preflight",
    "provision-seats",
    "owner-seat",
    "seal",
    "assemble",
];

/// The staging file Phase A/B write into the protected root and Phase C consumes.
/// Public key material and lineage digests only — never private material. It is
/// removed on a successful seal, so a completed ceremony leaves only the sealed
/// receipt behind.
pub const CEREMONY_STAGING_FILE: &str = "custody-seats.staged.json";

/// Schema of the authority-assembly manifest the `assemble` verb emits. Pinned by
/// `scripts/benchmark/m1nd10_g6_blind_runner.py` (`AUTHORITY_ASSEMBLY_SCHEMA`);
/// this is blocker 2 of `docs/benchmarks/G6-FORMAL-CEREMONY.md` §8.
pub const G6_AUTHORITY_ASSEMBLY_SCHEMA: &str = "m1nd10-g6-authority-assembly-v1";

/// The exact field set the G6 runner requires (`AUTHORITY_ASSEMBLY_FIELDS`). The
/// runner validates the set EXACTLY and recomputes the manifest's self digest, so
/// a missing or extra field is a hard refusal at the owner's one sealed run.
pub const G6_AUTHORITY_ASSEMBLY_MANIFEST_FIELDS: &[&str] = &[
    "schema",
    "assembly_id",
    "provider_kind",
    "production_authority_assembly",
    "owner_binary_digest",
    "provider_executable_digest",
    "owner_security_config_digest",
    "verification_key_registry",
    "receipt_key_id",
    "max_future_clock_skew_ms",
    "self_digest",
];

/// One step of the ceremony.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustodyCeremonyVerbV1 {
    /// Report every prerequisite. Provisions nothing, seals nothing, creates
    /// nothing. The only verb an agent may run.
    Preflight,
    /// Phase A — provision the four unattended verifier seats.
    ProvisionSeats,
    /// Phase B — the owner's biometric seat. Owner only, irreducibly.
    OwnerSeat,
    /// Phase C — build, validate, bind and enclave-seal the ceremony receipt.
    Seal,
    /// Assemble the production owner authority from the sealed ceremony and emit
    /// the pinned authority manifest. This is the non-test caller of
    /// `assemble_production_owner_authority_v1`.
    Assemble,
}

impl CustodyCeremonyVerbV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preflight => "preflight",
            Self::ProvisionSeats => "provision-seats",
            Self::OwnerSeat => "owner-seat",
            Self::Seal => "seal",
            Self::Assemble => "assemble",
        }
    }

    /// Whether the step needs the owner's body. Exactly one does: the biometric
    /// seat. The real gate is `kSecAccessControlUserPresence`, enforced by the
    /// enclave; this flag is what lets the CLI refuse BEFORE blocking on a Touch ID
    /// prompt nobody is present to answer.
    pub fn requires_owner_presence(self) -> bool {
        matches!(self, Self::OwnerSeat)
    }

    /// Whether the step mints or seals custody material. `preflight` and
    /// `assemble` are read-only with respect to custody state.
    pub fn mutates_custody(self) -> bool {
        matches!(self, Self::ProvisionSeats | Self::OwnerSeat | Self::Seal)
    }
}

impl FromStr for CustodyCeremonyVerbV1 {
    type Err = CeremonyRefusalV1;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "preflight" => Ok(Self::Preflight),
            "provision-seats" => Ok(Self::ProvisionSeats),
            "owner-seat" => Ok(Self::OwnerSeat),
            "seal" => Ok(Self::Seal),
            "assemble" => Ok(Self::Assemble),
            other => Err(CeremonyRefusalV1::UnknownVerb(other.to_owned())),
        }
    }
}

/// Whether a human is present at this process. Not a security boundary — see
/// [`CustodyCeremonyVerbV1::requires_owner_presence`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CeremonyAttendanceV1 {
    InteractiveTerminal,
    Unattended,
}

impl CeremonyAttendanceV1 {
    /// A ceremony driven by a human is attached to a terminal. A scheduler, CI job
    /// or agent harness is not.
    ///
    /// `std::io::IsTerminal` rather than a `libc::isatty` call: it is stdlib, needs
    /// no `unsafe`, and is correct on all three targets — the Windows console is
    /// not a tty in the unix sense, so a hand-rolled unix-only probe would report
    /// every Windows operator as unattended.
    pub fn detect() -> Self {
        use std::io::IsTerminal as _;

        if std::io::stdin().is_terminal() {
            Self::InteractiveTerminal
        } else {
            Self::Unattended
        }
    }
}

// ===========================================================================
// The stamp. THE ONLY admission to a custody ceremony.
// ===========================================================================

/// THE STAMP — the owner's own ceremony ingress, `m1nd-mcp --custody-ceremony`.
///
/// A value the owner constructs about ITSELF, never data that arrives. It carries
/// no public field and is built at exactly one site: the CLI dispatch in
/// `main.rs`. [`run_custody_ceremony`] takes it by value, so the ceremony is
/// unreachable without one. A payload field named `origin`, `ceremony_via` or
/// anything else is read by no code path in this crate.
pub struct OwnerCeremonyIngressV1 {
    _closed: (),
}

impl OwnerCeremonyIngressV1 {
    /// Construct the stamp from the ONLY fact that mints it: the human ran the
    /// ceremony command. Called from `main.rs` and nowhere else — the battery
    /// holds that line mechanically.
    pub fn from_cli_ingress() -> Self {
        Self { _closed: () }
    }
}

// ===========================================================================
// Refusals — a closed set, each naming a cause the owner can act on
// ===========================================================================

/// Why a ceremony step refused. Every variant names a cause with an owner-facing
/// remedy; none of them is an opaque platform code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CeremonyRefusalV1 {
    /// Not one of [`CUSTODY_CEREMONY_VERBS`].
    UnknownVerb(String),
    /// The custody floor is absent by construction on this target. Never a
    /// software fallback — that is the point of the Path-B floor.
    NotInstalledOnThisPlatform,
    /// The biometric step was invoked by a process with no human attached.
    UnattendedOwnerPresenceRefused,
    /// The binary is not codesigned with the `KeychainAccessGroups` entitlement,
    /// so a Secure Enclave key can neither be persisted nor resolved (§1 P4).
    KeychainEntitlementMissing,
    /// The protected root is missing, not `0700`, a symlink, or unreadable.
    ProtectedRootUnusable(String),
    /// An earlier phase has not completed, so this one has nothing to consume.
    CeremonyIncomplete(String),
    /// This phase already ran. Provisioning is never open-or-create, so a second
    /// run refuses instead of minting a second set of keys over the first.
    SeatsAlreadyStaged(String),
    /// The ceremony is complete but what it would seal is not a valid receipt —
    /// a malformed digest, seats that are not the presented spec's, or a sealing
    /// key that is not the one this ceremony provisioned. Nothing is written.
    CeremonyReceiptInvalid(String),
    /// The platform refused for some other reason, reported verbatim.
    PlatformRefused(String),
}

impl CeremonyRefusalV1 {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownVerb(_) => "custody_ceremony_unknown_verb",
            Self::NotInstalledOnThisPlatform => "custody_ceremony_not_installed",
            Self::UnattendedOwnerPresenceRefused => "custody_ceremony_unattended_presence_refused",
            Self::KeychainEntitlementMissing => "custody_ceremony_keychain_entitlement_missing",
            Self::ProtectedRootUnusable(_) => "custody_ceremony_protected_root_unusable",
            Self::CeremonyIncomplete(_) => "custody_ceremony_incomplete",
            Self::SeatsAlreadyStaged(_) => "custody_ceremony_seats_already_staged",
            Self::CeremonyReceiptInvalid(_) => "custody_ceremony_receipt_invalid",
            Self::PlatformRefused(_) => "custody_ceremony_platform_refused",
        }
    }

    /// The refusal as the one closed JSON object every one-shot mode prints.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": "m1nd-custody-ceremony-refusal-v1",
            "status": "REFUSED",
            "code": self.code(),
            "detail": self.to_string(),
        })
    }
}

impl fmt::Display for CeremonyRefusalV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownVerb(verb) => write!(
                formatter,
                "unknown custody ceremony verb '{verb}'; expected one of {}",
                CUSTODY_CEREMONY_VERBS.join(", ")
            ),
            Self::NotInstalledOnThisPlatform => write!(
                formatter,
                "the Secure Enclave custody floor is not installed on this target; \
                 it is macOS-only by construction and never falls back to software assurance"
            ),
            Self::UnattendedOwnerPresenceRefused => write!(
                formatter,
                "the owner's biometric seat requires the owner present at a terminal; \
                 this process has no human attached"
            ),
            Self::KeychainEntitlementMissing => write!(
                formatter,
                "this binary is not codesigned with the KeychainAccessGroups entitlement, so a \
                 Secure Enclave key cannot be persisted or resolved (prerequisite P4); \
                 see build/README.md"
            ),
            Self::ProtectedRootUnusable(detail) => {
                write!(formatter, "protected root unusable: {detail}")
            }
            Self::CeremonyIncomplete(detail) => {
                write!(formatter, "ceremony incomplete: {detail}")
            }
            Self::SeatsAlreadyStaged(detail) => {
                write!(formatter, "ceremony already staged: {detail}")
            }
            Self::CeremonyReceiptInvalid(detail) => {
                write!(formatter, "ceremony receipt invalid: {detail}")
            }
            Self::PlatformRefused(detail) => write!(formatter, "platform refused: {detail}"),
        }
    }
}

impl std::error::Error for CeremonyRefusalV1 {}

/// Admit or refuse a step on policy alone, before any platform call. Two
/// independent gates: the biometric step must have a human attached, and the floor
/// must exist on this target. A step passes only when BOTH admit it, so the order
/// below decides which CAUSE is reported, never whether the step runs.
///
/// **Presence is asked first, and it is asked on every target.** Who invoked the
/// ceremony is a fact about the CALLER; whether the enclave floor is compiled in is
/// a fact about the HOST. An unattended process is refused the owner's biometric
/// seat everywhere, because the claim it makes — that the owner is present — is
/// false on Linux and Windows exactly as it is false on macOS. Asking the host
/// question first made the presence refusal unreachable off macOS and reported
/// "not installed here", which reads as *this would have proceeded on a Mac* — the
/// weaker of the two true answers, and the one that contradicts the surfaces that
/// promise the refusal without qualification (`AGENTS.md` § the custody ceremony,
/// the `--custody-ceremony` help in `cli.rs`, and the battery's own §2: an
/// unattended process must refuse BEFORE reaching the enclave).
pub fn authorize_ceremony_step(
    verb: CustodyCeremonyVerbV1,
    attendance: CeremonyAttendanceV1,
) -> Result<(), CeremonyRefusalV1> {
    if verb.requires_owner_presence() && attendance == CeremonyAttendanceV1::Unattended {
        return Err(CeremonyRefusalV1::UnattendedOwnerPresenceRefused);
    }
    // Preflight is how an operator LEARNS the floor is unavailable here, so it
    // stays runnable on every target.
    if !cfg!(target_os = "macos") && verb != CustodyCeremonyVerbV1::Preflight {
        return Err(CeremonyRefusalV1::NotInstalledOnThisPlatform);
    }
    Ok(())
}

/// Turn a Security.framework failure into a refusal that names a cause the owner
/// can act on. The entitlement case is singled out because it is the one that is
/// invisible until the ceremony fails (§5 R1): an unentitled binary cannot write
/// to or read from the data-protection keychain at all, so provision, open and
/// sign all fail with codes that say nothing about the real cause.
pub fn classify_provisioning_failure(platform_error: &str) -> CeremonyRefusalV1 {
    let lowered = platform_error.to_lowercase();
    // errSecMissingEntitlement == -34018.
    if lowered.contains("-34018")
        || lowered.contains("errsecmissingentitlement")
        || lowered.contains("entitlement")
    {
        return CeremonyRefusalV1::KeychainEntitlementMissing;
    }
    CeremonyRefusalV1::PlatformRefused(platform_error.to_owned())
}

// ===========================================================================
// Preflight — the one safe step. Reports, never provisions.
// ===========================================================================

/// One prerequisite row (`G9-CUSTODY-CEREMONY.md` §1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreflightCheckV1 {
    pub id: String,
    /// `PASS`, `FAIL`, `OWNER` (only the owner can observe it) or `UNPROVEN`
    /// (knowable only at ceremony time — checking it here would touch custody).
    pub state: String,
    pub detail: String,
}

/// The preflight report. Creates nothing: an operator can run this on any machine,
/// any platform, at any time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreflightReportV1 {
    pub checks: Vec<PreflightCheckV1>,
    /// True when no check is in state `FAIL`. `OWNER` and `UNPROVEN` rows do not
    /// block readiness — they are what the ceremony itself proves.
    pub ready: bool,
}

impl PreflightReportV1 {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": "m1nd-custody-ceremony-preflight-v1",
            "ready": self.ready,
            "checks": self.checks,
        })
    }
}

fn check(id: &str, state: &str, detail: &str) -> PreflightCheckV1 {
    PreflightCheckV1 {
        id: id.to_owned(),
        state: state.to_owned(),
        detail: detail.to_owned(),
    }
}

/// Inspect the protected root WITHOUT creating or modifying it. Refusing to
/// provision the owner's own prerequisite is deliberate: creating it here would
/// hide a step of the ceremony from the person running it.
fn protected_root_check(protected_root: &Path) -> PreflightCheckV1 {
    let metadata = match fs::symlink_metadata(protected_root) {
        Ok(metadata) => metadata,
        Err(error) => {
            return check(
                "P5",
                "FAIL",
                &format!(
                    "{}: {error} — create it 0700 before the ceremony",
                    protected_root.display()
                ),
            );
        }
    };
    if metadata.file_type().is_symlink() {
        return check(
            "P5",
            "FAIL",
            "the protected root is a symlink; sealed slots are pinned by device/inode",
        );
    }
    if !metadata.is_dir() {
        return check("P5", "FAIL", "the protected root is not a directory");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode() & 0o777;
        if mode != 0o700 {
            return check(
                "P5",
                "FAIL",
                &format!("mode is {mode:o}, must be 700 (owner-only)"),
            );
        }
    }
    check(
        "P5",
        "PASS",
        &format!("{} is an owner-only directory", protected_root.display()),
    )
}

/// Report every prerequisite in `G9-CUSTODY-CEREMONY.md` §1. Provisions nothing.
pub fn preflight(protected_root: &Path) -> PreflightReportV1 {
    let checks = vec![
        check(
            "P1",
            "OWNER",
            "Apple Silicon / T2 Mac with a Secure Enclave, owner physically present — \
             only the owner can observe this",
        ),
        check(
            "P2",
            "OWNER",
            "Touch ID enrolled for the owner — only the owner can observe this",
        ),
        if cfg!(target_os = "macos") {
            check(
                "P3",
                "PASS",
                "macOS target: the custody floor is compiled in",
            )
        } else {
            check(
                "P3",
                "FAIL",
                "non-macOS target: the custody floor is absent by construction and fails \
                 closed rather than falling back to software",
            )
        },
        check(
            "P4",
            "UNPROVEN",
            "KeychainAccessGroups entitlement — proven only by the ceremony itself. The \
             release signs with build/m1nd-mcp.entitlements.plist; a locally built binary \
             does not, and will fail closed naming this prerequisite",
        ),
        protected_root_check(protected_root),
        check(
            "P6",
            "PASS",
            "custody dependency pins compiled in: security-framework =3.7.0, \
             security-framework-sys =2.17.0, core-foundation =0.10.1 — custody surface, \
             never bumped opportunistically",
        ),
        check(
            "P7",
            "PASS",
            "crypto stack coherent after the RustCrypto sweep; P-256 verification runs \
             through m1nd-control's verifier",
        ),
        check(
            "P8",
            "PASS",
            "ceremony surface present: m1nd-mcp --custody-ceremony <verb>",
        ),
    ];
    let ready = !checks.iter().any(|check| check.state == "FAIL");
    PreflightReportV1 { checks, ready }
}

// ===========================================================================
// Phase C's precondition — a partial ceremony commits nothing
// ===========================================================================

/// Schema of the staging file. Its own, not the sealed receipt's: what Phase A
/// and Phase B write is a work-in-progress record, and nothing downstream may
/// mistake it for a ceremony.
pub const CEREMONY_STAGING_SCHEMA: &str = "m1nd-custody-ceremony-staged-v1";

/// One staged verifier seat: the seat's identity as the owner's independence spec
/// declares it, plus the PUBLIC half of the enclave key minted for it and the
/// lineage digest its provisioning permit was bound to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StagedSeatV1 {
    pub principal_id: String,
    pub key_id: String,
    pub failure_domain: String,
    /// 65-byte uncompressed SEC1 P-256 point, lowercase hex (§3).
    pub public_key: String,
    /// The provisioning permit's `bound_context_digest` — the ceremony lineage a
    /// seat cannot be lifted out of.
    pub bound_context_digest: String,
}

/// The ceremony's own sealing seat. Deliberately NOT a [`StagedSeatV1`]: it casts
/// no vote, holds no failure domain, and must never be mistaken for one of the
/// four. Its enclave key is what signs the sealed slots in the protected root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StagedSealingSeatV1 {
    pub key_id: String,
    pub public_key: String,
}

/// What Phase A and Phase B stage for Phase C to seal. Public key material and
/// lineage digests only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StagedCeremonyV1 {
    pub schema: String,
    /// The four unattended verifier seats, in the order the independence spec
    /// declares them.
    pub verifier_seats: Vec<StagedSeatV1>,
    /// The seat whose enclave key seals the protected root, staged by Phase A.
    #[serde(default)]
    pub sealing_seat: Option<StagedSealingSeatV1>,
    /// The owner's biometric seat public key, once Phase B has run.
    pub owner_biometric_seat_public_key: Option<String>,
}

/// Read the staged ceremony, or say which step has not run yet.
fn read_staged_ceremony(protected_root: &Path) -> Result<StagedCeremonyV1, CeremonyRefusalV1> {
    let path = protected_root.join(CEREMONY_STAGING_FILE);
    let bytes = fs::read(&path).map_err(|error| {
        CeremonyRefusalV1::CeremonyIncomplete(format!(
            "no staged ceremony at {}: run --custody-ceremony provision-seats first ({error})",
            path.display()
        ))
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        CeremonyRefusalV1::CeremonyIncomplete(format!("staged ceremony is unreadable: {error}"))
    })
}

/// Refuse to seal unless BOTH earlier phases completed. Reads only; a refusal here
/// leaves the protected root exactly as it was, because a half-sealed custody root
/// is worse than an unsealed one — it looks finished.
pub fn seal_requires_a_complete_ceremony(
    protected_root: &Path,
) -> Result<StagedCeremonyV1, CeremonyRefusalV1> {
    let staged = read_staged_ceremony(protected_root)?;
    if staged.verifier_seats.len() != 4 {
        return Err(CeremonyRefusalV1::CeremonyIncomplete(format!(
            "expected 4 staged verifier seats, found {}",
            staged.verifier_seats.len()
        )));
    }
    if staged.sealing_seat.is_none() {
        return Err(CeremonyRefusalV1::CeremonyIncomplete(
            "the ceremony's sealing seat has not been provisioned: \
             run --custody-ceremony provision-seats"
                .to_owned(),
        ));
    }
    if staged.owner_biometric_seat_public_key.is_none() {
        return Err(CeremonyRefusalV1::CeremonyIncomplete(
            "the owner's biometric seat has not been provisioned: \
             run --custody-ceremony owner-seat"
                .to_owned(),
        ));
    }
    Ok(staged)
}

// ===========================================================================
// The seam — assembling the production owner authority
// ===========================================================================

/// Inputs the ceremony's steps need. Every path is owner-held: this binary holds
/// none of them and derives none of them, exactly as the G6 ceremony's script does.
#[derive(Clone, Debug)]
pub struct CeremonyRequestV1 {
    pub verb: CustodyCeremonyVerbV1,
    pub protected_root: std::path::PathBuf,
    pub owner_security_config: Option<std::path::PathBuf>,
    pub mission_config: Option<std::path::PathBuf>,
    /// The owner's `IndependenceSpecV1`, whose four voting seats Phase A
    /// provisions and Phase C seals. Owner-held because the seats belong to the
    /// constitution, not to this process.
    pub independence_spec: Option<std::path::PathBuf>,
    /// The owner's constitution digest, sealed into the ceremony receipt.
    pub constitution_digest: Option<String>,
}

/// Run one ceremony step and return its closed JSON object plus a process exit
/// code. Takes the stamp BY VALUE: unreachable without the CLI ingress.
pub fn run_custody_ceremony(
    _ingress: OwnerCeremonyIngressV1,
    request: CeremonyRequestV1,
    attendance: CeremonyAttendanceV1,
) -> (serde_json::Value, i32) {
    if let Err(refusal) = authorize_ceremony_step(request.verb, attendance) {
        return (refusal.to_json(), 1);
    }
    match request.verb {
        CustodyCeremonyVerbV1::Preflight => {
            let report = preflight(&request.protected_root);
            let code = i32::from(!report.ready);
            (report.to_json(), code)
        }
        CustodyCeremonyVerbV1::ProvisionSeats => provision_seats_verb(&request),
        CustodyCeremonyVerbV1::OwnerSeat => owner_seat_verb(&request),
        CustodyCeremonyVerbV1::Seal => seal_verb(&request),
        CustodyCeremonyVerbV1::Assemble => assemble_verb(&request),
    }
}

// ===========================================================================
// Off macOS the floor is absent by construction. `authorize_ceremony_step`
// already refuses every custody verb here — these arms exist so the binary
// still COMPILES on the other two CI legs, and they refuse identically rather
// than selecting a software fallback.
// ===========================================================================

#[cfg(not(target_os = "macos"))]
fn provision_seats_verb(_request: &CeremonyRequestV1) -> (serde_json::Value, i32) {
    (CeremonyRefusalV1::NotInstalledOnThisPlatform.to_json(), 1)
}

#[cfg(not(target_os = "macos"))]
fn owner_seat_verb(_request: &CeremonyRequestV1) -> (serde_json::Value, i32) {
    (CeremonyRefusalV1::NotInstalledOnThisPlatform.to_json(), 1)
}

#[cfg(not(target_os = "macos"))]
fn seal_verb(_request: &CeremonyRequestV1) -> (serde_json::Value, i32) {
    (CeremonyRefusalV1::NotInstalledOnThisPlatform.to_json(), 1)
}

#[cfg(not(target_os = "macos"))]
fn assemble_verb(_request: &CeremonyRequestV1) -> (serde_json::Value, i32) {
    (CeremonyRefusalV1::NotInstalledOnThisPlatform.to_json(), 1)
}

// ===========================================================================
// Phase A, B and C against the real floor (macOS).
//
// Each verb constructs the production key store, does ONE bounded thing, and
// prints one closed JSON object. The store is injected into the functions that
// do the work, which is the same narrow boundary `SecureEnclaveKeyStoreV1`
// already exists to be — it is what lets the battery prove the custody path
// without hardware, and it is not a bypass: the verbs below pass the real store
// and nothing else can call them.
// ===========================================================================

/// The wall-clock the ceremony stamps its receipt with.
#[cfg(target_os = "macos")]
fn ceremony_clock_ms() -> Result<u64, CeremonyRefusalV1> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .map_err(|error| CeremonyRefusalV1::PlatformRefused(error.to_string()))
}

/// The protected root is the OWNER's prerequisite (P5). Refuse before touching
/// the keychain so a misconfigured root never reaches it.
fn require_owner_only_protected_root(protected_root: &Path) -> Result<(), CeremonyRefusalV1> {
    let check = protected_root_check(protected_root);
    if check.state == "FAIL" {
        return Err(CeremonyRefusalV1::ProtectedRootUnusable(check.detail));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn custody_store(
    access_control: crate::enclave_authority::EnclaveAccessControlV1,
) -> std::sync::Arc<dyn crate::enclave_authority::SecureEnclaveKeyStoreV1> {
    std::sync::Arc::new(
        crate::enclave_authority::SecurityFrameworkEnclaveKeyStore::new(
            CUSTODY_KEYCHAIN_LABEL_PREFIX,
            CUSTODY_SUBJECT_ID,
            access_control,
        ),
    )
}

/// The permit one seat is provisioned under. `bound_context_digest` is the
/// ceremony's lineage — the digest of the independence spec whose seats these
/// are — so a key minted for one ceremony cannot be sealed into another.
#[cfg(target_os = "macos")]
fn seat_permit(
    key_id: &str,
    access_control: crate::enclave_authority::EnclaveAccessControlV1,
    bound_context_digest: &str,
) -> crate::enclave_authority::EnclaveProvisioningPermitV1 {
    crate::enclave_authority::EnclaveProvisioningPermitV1 {
        key_id: key_id.to_owned(),
        subject_id: CUSTODY_SUBJECT_ID.to_owned(),
        application_label: format!("{CUSTODY_KEYCHAIN_LABEL_PREFIX}.{key_id}"),
        access_control,
        bound_context_digest: bound_context_digest.to_owned(),
    }
}

/// Publish the staging file into the protected root through the floor's own
/// atomic, no-follow, `0600` write — the staged record holds only public
/// material, but it lives in the owner's custody root and is written like it.
#[cfg(target_os = "macos")]
fn write_staged_ceremony(
    protected_root: &Path,
    staged: &StagedCeremonyV1,
) -> Result<(), CeremonyRefusalV1> {
    let bytes = serde_json::to_vec_pretty(staged)
        .map_err(|error| CeremonyRefusalV1::PlatformRefused(error.to_string()))?;
    crate::enclave_authority::atomic_write_no_follow(protected_root, CEREMONY_STAGING_FILE, &bytes)
        .map_err(|error| CeremonyRefusalV1::ProtectedRootUnusable(error.to_string()))
}

/// The one lineage every staged seat shares.
fn ceremony_lineage(staged: &StagedCeremonyV1) -> Result<String, CeremonyRefusalV1> {
    let first = staged.verifier_seats.first().ok_or_else(|| {
        CeremonyRefusalV1::CeremonyIncomplete(
            "no verifier seat is staged: run --custody-ceremony provision-seats first".to_owned(),
        )
    })?;
    if staged
        .verifier_seats
        .iter()
        .any(|seat| seat.bound_context_digest != first.bound_context_digest)
    {
        return Err(CeremonyRefusalV1::CeremonyReceiptInvalid(
            "the staged verifier seats carry more than one ceremony lineage".to_owned(),
        ));
    }
    Ok(first.bound_context_digest.clone())
}

/// Read the owner's independence spec. It is required: the seats are the
/// constitution's, and this binary never invents one.
#[cfg(target_os = "macos")]
fn load_independence_spec(
    path: Option<&std::path::PathBuf>,
) -> Result<m1nd_control::autonomy::IndependenceSpecV1, CeremonyRefusalV1> {
    let path = path.ok_or_else(|| {
        CeremonyRefusalV1::CeremonyIncomplete(
            "--custody-independence-spec is required: the four verifier seats are the \
             constitution's own voting seats, and this binary never invents one"
                .to_owned(),
        )
    })?;
    let bytes = fs::read(path).map_err(|error| {
        CeremonyRefusalV1::CeremonyIncomplete(format!("{}: {error}", path.display()))
    })?;
    let spec: m1nd_control::autonomy::IndependenceSpecV1 =
        serde_json::from_slice(&bytes).map_err(|error| {
            CeremonyRefusalV1::CeremonyIncomplete(format!(
                "independence spec is unreadable: {error}"
            ))
        })?;
    require_a_usable_independence_spec(&spec)?;
    Ok(spec)
}

/// The spec must be internally honest before four enclave keys are bound to it:
/// its declared digest must be the digest of its own core, and it must carry the
/// frozen counts (`IMMUTABLE_VERIFIER_SEATS`, `IMMUTABLE_FAILURE_DOMAINS`).
/// Checked BEFORE the first key is minted — discovering it afterwards would leave
/// orphaned keys in the keychain that only a re-provisioning ceremony can clear.
#[cfg(target_os = "macos")]
fn require_a_usable_independence_spec(
    spec: &m1nd_control::autonomy::IndependenceSpecV1,
) -> Result<(), CeremonyRefusalV1> {
    use m1nd_control::autonomy::{
        IMMUTABLE_FAILURE_DOMAINS, IMMUTABLE_VERIFIER_SEATS, INDEPENDENCE_SPEC_SCHEMA,
    };

    if spec.schema != INDEPENDENCE_SPEC_SCHEMA {
        return Err(CeremonyRefusalV1::CeremonyIncomplete(format!(
            "independence spec schema is '{}', expected '{INDEPENDENCE_SPEC_SCHEMA}'",
            spec.schema
        )));
    }
    let recomputed = spec.compute_digest().map_err(|error| {
        CeremonyRefusalV1::CeremonyIncomplete(format!(
            "independence spec is not canonical: {error}"
        ))
    })?;
    if recomputed != spec.independence_spec_digest {
        return Err(CeremonyRefusalV1::CeremonyIncomplete(format!(
            "the independence spec declares digest {} but its core digests to {recomputed}",
            spec.independence_spec_digest
        )));
    }
    if spec.core.voting_verifiers.len() != usize::from(IMMUTABLE_VERIFIER_SEATS) {
        return Err(CeremonyRefusalV1::CeremonyIncomplete(format!(
            "the independence spec names {} voting seats; the ceremony provisions exactly \
             {IMMUTABLE_VERIFIER_SEATS}",
            spec.core.voting_verifiers.len()
        )));
    }
    let domains: std::collections::BTreeSet<&str> = spec
        .core
        .voting_verifiers
        .iter()
        .map(|seat| seat.failure_domain.as_str())
        .collect();
    if domains.len() < usize::from(IMMUTABLE_FAILURE_DOMAINS) {
        return Err(CeremonyRefusalV1::CeremonyIncomplete(format!(
            "the independence spec spans {} failure domains; the ceremony requires at least \
             {IMMUTABLE_FAILURE_DOMAINS}",
            domains.len()
        )));
    }
    Ok(())
}

/// **Phase A, steps 1-2.** Mint one non-exportable enclave key per voting seat the
/// spec names, plus the ceremony's own sealing seat, and stage their public halves.
///
/// Idempotence is a REFUSAL, not a no-op: the floor's provisioning is never
/// open-or-create (an existing `kSecAttrLabel` fails closed), so a second run that
/// quietly re-staged would either contradict the keychain or orphan the first set
/// of keys. Retiring seats is an owner operational step (§2 step 8).
#[cfg(target_os = "macos")]
pub(crate) fn provision_seats_into_store(
    unattended_store: &dyn crate::enclave_authority::SecureEnclaveKeyStoreV1,
    spec: &m1nd_control::autonomy::IndependenceSpecV1,
    protected_root: &Path,
) -> Result<StagedCeremonyV1, CeremonyRefusalV1> {
    use crate::enclave_authority::{
        hex_lower, provision_agent_enclave_seat, EnclaveAccessControlV1,
    };

    require_a_usable_independence_spec(spec)?;
    require_owner_only_protected_root(protected_root)?;
    let staging = protected_root.join(CEREMONY_STAGING_FILE);
    if staging.exists() {
        return Err(CeremonyRefusalV1::SeatsAlreadyStaged(format!(
            "a ceremony is already staged at {}; provisioning is never open-or-create, so the \
             owner retires the existing seats before staging new ones",
            staging.display()
        )));
    }

    let lineage = &spec.independence_spec_digest;
    let mut verifier_seats = Vec::with_capacity(spec.core.voting_verifiers.len());
    for seat in &spec.core.voting_verifiers {
        let permit = seat_permit(
            &seat.key_id,
            EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
            lineage,
        );
        let opened = provision_agent_enclave_seat(unattended_store, &permit)
            .map_err(|error| classify_provisioning_failure(&error.to_string()))?;
        verifier_seats.push(StagedSeatV1 {
            principal_id: seat.principal_id.clone(),
            key_id: seat.key_id.clone(),
            failure_domain: seat.failure_domain.clone(),
            public_key: hex_lower(&opened.public_key_sec1),
            bound_context_digest: lineage.clone(),
        });
    }

    let sealing = provision_agent_enclave_seat(
        unattended_store,
        &seat_permit(
            CUSTODY_SEALING_SEAT_KEY_ID,
            EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
            lineage,
        ),
    )
    .map_err(|error| classify_provisioning_failure(&error.to_string()))?;

    let staged = StagedCeremonyV1 {
        schema: CEREMONY_STAGING_SCHEMA.to_owned(),
        verifier_seats,
        sealing_seat: Some(StagedSealingSeatV1 {
            key_id: CUSTODY_SEALING_SEAT_KEY_ID.to_owned(),
            public_key: hex_lower(&sealing.public_key_sec1),
        }),
        owner_biometric_seat_public_key: None,
    };
    write_staged_ceremony(protected_root, &staged)?;
    Ok(staged)
}

/// **Phase B, step 4.** The owner's biometric seat — the `owner_signature`
/// authority that stays present even under `AgentQuorum`, and never a voting seat.
///
/// It goes through `provision_owner_biometric_seat`, the floor's owner-only entry
/// point, which refuses any non-biometric class exactly as its agent-side mirror
/// refuses the biometric one. Minting does not raise a Touch ID prompt: the
/// key's `kSecAccessControl` gates its USE, and that gate is the enclave's, not
/// this code's.
#[cfg(target_os = "macos")]
pub(crate) fn provision_owner_seat_into_store(
    biometric_store: &dyn crate::enclave_authority::SecureEnclaveKeyStoreV1,
    protected_root: &Path,
) -> Result<StagedCeremonyV1, CeremonyRefusalV1> {
    use crate::enclave_authority::{
        hex_lower, provision_owner_biometric_seat, EnclaveAccessControlV1,
    };

    require_owner_only_protected_root(protected_root)?;
    let mut staged = read_staged_ceremony(protected_root)?;
    if staged.owner_biometric_seat_public_key.is_some() {
        return Err(CeremonyRefusalV1::SeatsAlreadyStaged(
            "the owner's biometric seat is already staged; it is minted once per ceremony"
                .to_owned(),
        ));
    }
    let lineage = ceremony_lineage(&staged)?;
    let opened = provision_owner_biometric_seat(
        biometric_store,
        &seat_permit(
            CUSTODY_OWNER_BIOMETRIC_SEAT_KEY_ID,
            EnclaveAccessControlV1::UserPresenceBiometricNonExportable,
            &lineage,
        ),
    )
    .map_err(|error| classify_provisioning_failure(&error.to_string()))?;

    let public_key = hex_lower(&opened.public_key_sec1);
    if staged
        .verifier_seats
        .iter()
        .any(|seat| seat.public_key == public_key)
    {
        return Err(CeremonyRefusalV1::CeremonyReceiptInvalid(
            "the owner's biometric seat resolved to a key that is already a voting seat; \
             owner_signature is never a quorum seat"
                .to_owned(),
        ));
    }
    staged.owner_biometric_seat_public_key = Some(public_key);
    write_staged_ceremony(protected_root, &staged)?;
    Ok(staged)
}

/// The ceremony root, its sealing key and the signer behind it — opened ONCE, by
/// the one function both `seal` and `assemble` call. That is what makes "assemble
/// reads back what seal wrote" true by construction rather than by coincidence:
/// the sealed slot's identity, root binding and context digest are all derived
/// here, so the two steps cannot drift apart.
#[cfg(target_os = "macos")]
type CeremonyRootHandleV1 = (
    crate::enclave_authority::SealedProtectedRootV1,
    m1nd_control::VerificationKeyV1,
    std::sync::Arc<dyn m1nd_control::AuthoritySigner + Send + Sync>,
);

#[cfg(target_os = "macos")]
pub(crate) fn open_ceremony_root(
    key_store: std::sync::Arc<dyn crate::enclave_authority::SecureEnclaveKeyStoreV1>,
    protected_root: &Path,
    now_ms: u64,
) -> Result<CeremonyRootHandleV1, CeremonyRefusalV1> {
    use std::sync::Arc;

    use crate::enclave_authority::{
        EnclaveAccessControlV1, EnclaveKeyAttestationV1, SealedProtectedRootV1, SecureEnclaveSigner,
    };

    // An unentitled binary cannot resolve the key at all, which is where P4
    // becomes visible; the classifier names that prerequisite rather than an
    // opaque OSStatus.
    let signer = SecureEnclaveSigner::open_attested(
        key_store,
        CUSTODY_SEALING_SEAT_KEY_ID,
        &EnclaveKeyAttestationV1::canonical(EnclaveAccessControlV1::PrivateKeyUsageNonExportable),
    )
    .map_err(|error| classify_provisioning_failure(&error.to_string()))?;
    let verification_key = signer.verification_key(now_ms, now_ms);
    let signer: Arc<dyn m1nd_control::AuthoritySigner + Send + Sync> = Arc::new(signer);
    let root = SealedProtectedRootV1::open(
        protected_root,
        &verification_key.public_key,
        Arc::clone(&signer),
        verification_key.clone(),
    )
    .map_err(|error| CeremonyRefusalV1::ProtectedRootUnusable(error.to_string()))?;
    Ok((root, verification_key, signer))
}

/// **Phase C, steps 5-7.** Build the ceremony receipt from the complete staged
/// ceremony, validate and bind it, enclave-seal it into the protected root, and
/// read it back through the same root before declaring anything.
///
/// Nothing is written until every check has passed, because a half-sealed custody
/// root is worse than an unsealed one — it looks finished. On success the staging
/// file is consumed, so a completed ceremony leaves only the sealed receipt.
#[cfg(target_os = "macos")]
pub(crate) fn seal_with_store(
    unattended_store: std::sync::Arc<dyn crate::enclave_authority::SecureEnclaveKeyStoreV1>,
    spec: &m1nd_control::autonomy::IndependenceSpecV1,
    constitution_digest: &str,
    protected_root: &Path,
    now_ms: u64,
) -> Result<crate::enclave_authority::EnclaveCustodyCeremonyReceiptV1, CeremonyRefusalV1> {
    use crate::enclave_authority::{
        CeremonyVerifierSeatV1, CustodyAttestationDistinctionV1, EnclaveCustodyCeremonyReceiptV1,
        ENCLAVE_CUSTODY_CEREMONY_SCHEMA, SECURE_ENCLAVE_CUSTODY_FLOOR_V1,
    };

    require_a_usable_independence_spec(spec)?;
    require_owner_only_protected_root(protected_root)?;
    let staged = seal_requires_a_complete_ceremony(protected_root)?;
    let sealing_seat = staged
        .sealing_seat
        .as_ref()
        .expect("a complete ceremony carries its sealing seat");

    // Lineage: the seats in the keychain were bound to ONE spec at provisioning
    // time. Presenting a different one at seal is refused even when its seat set
    // happens to match.
    let lineage = ceremony_lineage(&staged)?;
    if lineage != spec.independence_spec_digest {
        return Err(CeremonyRefusalV1::CeremonyReceiptInvalid(format!(
            "the staged seats were provisioned under independence spec {lineage}, not the one \
             presented at seal ({})",
            spec.independence_spec_digest
        )));
    }

    let receipt = EnclaveCustodyCeremonyReceiptV1 {
        schema: ENCLAVE_CUSTODY_CEREMONY_SCHEMA.to_owned(),
        custody_floor: SECURE_ENCLAVE_CUSTODY_FLOOR_V1.to_owned(),
        attestation: CustodyAttestationDistinctionV1::secure_enclave_single_host(),
        verifier_seats: staged
            .verifier_seats
            .iter()
            .map(|seat| CeremonyVerifierSeatV1 {
                principal_id: seat.principal_id.clone(),
                key_id: seat.key_id.clone(),
                failure_domain: seat.failure_domain.clone(),
                public_key: seat.public_key.clone(),
                bound_context_digest: seat.bound_context_digest.clone(),
            })
            .collect(),
        owner_biometric_seat_public_key: staged
            .owner_biometric_seat_public_key
            .clone()
            .unwrap_or_default(),
        independence_spec_digest: spec.independence_spec_digest.clone(),
        constitution_digest: constitution_digest.to_owned(),
        sealed_at: now_ms,
    };
    receipt
        .validate()
        .map_err(|error| CeremonyRefusalV1::CeremonyReceiptInvalid(error.to_string()))?;
    receipt
        .bind_independence_spec(spec)
        .map_err(|error| CeremonyRefusalV1::CeremonyReceiptInvalid(error.to_string()))?;

    let (ceremony_root, verification_key, _signer) =
        open_ceremony_root(unattended_store, protected_root, now_ms)?;
    if verification_key.public_key != sealing_seat.public_key {
        return Err(CeremonyRefusalV1::CeremonyReceiptInvalid(format!(
            "the enclave key filed under the ceremony's sealing label is not the one this \
             ceremony provisioned (staged {}, resolved {})",
            sealing_seat.public_key, verification_key.public_key
        )));
    }

    ceremony_root
        .seal_custody_ceremony(&receipt)
        .map_err(|error| CeremonyRefusalV1::PlatformRefused(error.to_string()))?;
    // Re-open and read back before claiming anything (§2 step 7).
    let read_back = ceremony_root
        .read_custody_ceremony()
        .map_err(|error| CeremonyRefusalV1::PlatformRefused(error.to_string()))?
        .ok_or_else(|| {
            CeremonyRefusalV1::PlatformRefused(
                "the sealed ceremony did not read back from the protected root".to_owned(),
            )
        })?;
    if read_back != receipt {
        return Err(CeremonyRefusalV1::PlatformRefused(
            "the ceremony read back from the protected root is not the one sealed".to_owned(),
        ));
    }

    // The staging file has done its job; a completed ceremony leaves only the
    // sealed receipt behind.
    fs::remove_file(protected_root.join(CEREMONY_STAGING_FILE))
        .map_err(|error| CeremonyRefusalV1::ProtectedRootUnusable(error.to_string()))?;
    Ok(receipt)
}

#[cfg(target_os = "macos")]
fn staged_step_json(
    verb: &str,
    protected_root: &Path,
    staged: &StagedCeremonyV1,
) -> serde_json::Value {
    serde_json::json!({
        "schema": "m1nd-custody-ceremony-step-v1",
        "status": "STAGED",
        "verb": verb,
        "staging_file": protected_root.join(CEREMONY_STAGING_FILE).display().to_string(),
        "verifier_seats": staged.verifier_seats,
        "sealing_seat": staged.sealing_seat,
        "owner_biometric_seat_public_key": staged.owner_biometric_seat_public_key,
        "next": if staged.owner_biometric_seat_public_key.is_none() {
            "m1nd-mcp --custody-ceremony owner-seat"
        } else {
            "m1nd-mcp --custody-ceremony seal"
        },
    })
}

#[cfg(target_os = "macos")]
fn provision_seats_verb(request: &CeremonyRequestV1) -> (serde_json::Value, i32) {
    use crate::enclave_authority::EnclaveAccessControlV1;

    let outcome = load_independence_spec(request.independence_spec.as_ref()).and_then(|spec| {
        let store = custody_store(EnclaveAccessControlV1::PrivateKeyUsageNonExportable);
        provision_seats_into_store(store.as_ref(), &spec, &request.protected_root)
    });
    match outcome {
        Ok(staged) => (
            staged_step_json("provision-seats", &request.protected_root, &staged),
            0,
        ),
        Err(refusal) => (refusal.to_json(), 1),
    }
}

#[cfg(target_os = "macos")]
fn owner_seat_verb(request: &CeremonyRequestV1) -> (serde_json::Value, i32) {
    use crate::enclave_authority::EnclaveAccessControlV1;

    let store = custody_store(EnclaveAccessControlV1::UserPresenceBiometricNonExportable);
    match provision_owner_seat_into_store(store.as_ref(), &request.protected_root) {
        Ok(staged) => (
            staged_step_json("owner-seat", &request.protected_root, &staged),
            0,
        ),
        Err(refusal) => (refusal.to_json(), 1),
    }
}

#[cfg(target_os = "macos")]
fn seal_verb(request: &CeremonyRequestV1) -> (serde_json::Value, i32) {
    use crate::enclave_authority::EnclaveAccessControlV1;

    let outcome = (|| {
        let spec = load_independence_spec(request.independence_spec.as_ref())?;
        let constitution_digest = request.constitution_digest.as_deref().ok_or_else(|| {
            CeremonyRefusalV1::CeremonyIncomplete(
                "--custody-constitution-digest is required: the ceremony records the owner's \
                 constitution digest, it does not compute one"
                    .to_owned(),
            )
        })?;
        let now_ms = ceremony_clock_ms()?;
        let store = custody_store(EnclaveAccessControlV1::PrivateKeyUsageNonExportable);
        seal_with_store(
            store,
            &spec,
            constitution_digest,
            &request.protected_root,
            now_ms,
        )
    })();
    match outcome {
        Ok(receipt) => (
            serde_json::json!({
                "schema": "m1nd-custody-ceremony-step-v1",
                "status": "SEALED",
                "verb": "seal",
                "sealed_receipt": request
                    .protected_root
                    .join(crate::enclave_authority::ENCLAVE_CUSTODY_CEREMONY_SLOT_FILE)
                    .display()
                    .to_string(),
                "receipt": receipt,
                "next": "m1nd-mcp --custody-ceremony assemble",
            }),
            0,
        ),
        Err(refusal) => (refusal.to_json(), 1),
    }
}

/// Assemble the production owner authority from the SEALED ceremony and emit the
/// pinned authority manifest the G6 formal run requires.
///
/// This is the non-test caller `G9-CUSTODY-CEREMONY.md` §4 measured as absent and
/// §5 R3 named as blocking the ladder. It is one-shot: it assembles, prints, and
/// exits — it never serves the assembled owner, opens a port or takes a lease.
///
/// It fails closed at every layer it does not control: no sealed ceremony, no
/// entitled binary, no hardware-attested provider, no assembly.
#[cfg(target_os = "macos")]
fn assemble_verb(request: &CeremonyRequestV1) -> (serde_json::Value, i32) {
    match assemble_production_floor(request) {
        Ok(manifest) => (manifest, 0),
        Err(refusal) => (refusal.to_json(), 1),
    }
}

#[cfg(target_os = "macos")]
fn assemble_production_floor(
    request: &CeremonyRequestV1,
) -> Result<serde_json::Value, CeremonyRefusalV1> {
    use std::sync::Arc;

    use crate::enclave_authority::{
        EnclaveAccessControlV1, EnclaveBackedWalRecordCrypto, SealedProtectedRootV1,
        SecureEnclaveJournalHeadBackend, SecureEnclaveOwnerSecurityConfigRootBackend,
        SecureEnclaveProtectedEpochBackend,
    };
    use crate::owner_security_config::{
        assemble_production_owner_authority_v1, OwnerAuthorityStartupV1,
        OwnerSecurityConfigLoaderV1, ProductionOwnerAuthorityInputsV1,
    };

    let owner_security_config = request.owner_security_config.as_ref().ok_or_else(|| {
        CeremonyRefusalV1::CeremonyIncomplete(
            "--custody-owner-security-config is required to assemble the production authority"
                .to_owned(),
        )
    })?;
    let mission_config_path = request.mission_config.as_ref().ok_or_else(|| {
        CeremonyRefusalV1::CeremonyIncomplete(
            "--custody-mission-config is required to assemble the production authority".to_owned(),
        )
    })?;

    // The protected root must already be the owner's 0700 directory. Refuse before
    // touching the enclave so a misconfigured root never reaches the keychain.
    require_owner_only_protected_root(&request.protected_root)?;
    let now_ms = ceremony_clock_ms()?;

    // 1-3. Open the sealing seat, re-attest it, and open the ceremony root — the
    //      SAME function `seal` used to write the receipt, so what assemble reads
    //      back cannot drift from what the ceremony sealed. An unentitled binary
    //      cannot resolve the key at all, which is where P4 becomes visible.
    let (ceremony_root, verification_key, signer) = open_ceremony_root(
        custody_store(EnclaveAccessControlV1::PrivateKeyUsageNonExportable),
        &request.protected_root,
        now_ms,
    )?;

    // The other protected roots. Two independent roots, as the assembly's own
    // contract requires; the ceremony root above is where the sealed receipt lives.
    let open_root = |sub: &str| -> Result<SealedProtectedRootV1, CeremonyRefusalV1> {
        let path = request.protected_root.join(sub);
        SealedProtectedRootV1::open(
            &path,
            &verification_key.public_key,
            Arc::clone(&signer),
            verification_key.clone(),
        )
        .map_err(|error| {
            CeremonyRefusalV1::ProtectedRootUnusable(format!("{}: {error}", path.display()))
        })
    };

    // The ceremony must already be sealed. No sealed receipt, no floor.
    let receipt = ceremony_root
        .read_custody_ceremony()
        .map_err(|error| CeremonyRefusalV1::PlatformRefused(error.to_string()))?
        .ok_or_else(|| {
            CeremonyRefusalV1::CeremonyIncomplete(
                "no sealed custody ceremony in the protected root: the ceremony has not run"
                    .to_owned(),
            )
        })?;

    // 4. Build the four hardware-attested providers and load the owner security
    //    config through the enclave-sealed root.
    let config_backend = SecureEnclaveOwnerSecurityConfigRootBackend::new(open_root("config")?);
    let loaded_security = OwnerSecurityConfigLoaderV1::load_production(
        owner_security_config,
        &config_backend,
        now_ms,
    )
    .map_err(|error| CeremonyRefusalV1::PlatformRefused(error.to_string()))?;
    let owner_security_config_digest = loaded_security.config_digest().to_owned();

    let authority_epoch_backend = Box::new(SecureEnclaveProtectedEpochBackend::new(open_root(
        "runtime",
    )?));
    let wal_record_crypto = Arc::new(
        EnclaveBackedWalRecordCrypto::new(Arc::clone(&signer), verification_key.clone())
            .map_err(|error| CeremonyRefusalV1::PlatformRefused(error.to_string()))?,
    );
    let protected_journal_head: crate::protected_journal_head::SharedProtectedJournalHeadBackendV1 =
        Arc::new(parking_lot::Mutex::new(Box::new(
            SecureEnclaveJournalHeadBackend::new(open_root("journal")?),
        )));

    let mission_config_bytes = fs::read(mission_config_path).map_err(|error| {
        CeremonyRefusalV1::CeremonyIncomplete(format!("{}: {error}", mission_config_path.display()))
    })?;
    let mission_config = serde_json::from_slice(&mission_config_bytes).map_err(|error| {
        CeremonyRefusalV1::CeremonyIncomplete(format!("mission config is unreadable: {error}"))
    })?;

    // 5. THE SEAM. The production owner authority, assembled from hardware-
    //    attested providers — the call G9-CUSTODY-CEREMONY.md §4 measured as
    //    having no production caller.
    let assembly = assemble_production_owner_authority_v1(ProductionOwnerAuthorityInputsV1 {
        loaded_security,
        startup: OwnerAuthorityStartupV1::OpenExisting,
        authority_epoch_backend,
        mission_config,
        owner_clock: Arc::new(move || now_ms),
        wal_record_crypto,
        protected_journal_head,
    })
    .map_err(|error| CeremonyRefusalV1::PlatformRefused(format!("{error:?}")))?;
    drop(assembly);

    // 6. Emit the manifest the G6 formal run requires, carrying the runner's EXACT
    //    field set (a missing or extra key is a hard refusal there).
    //
    //    Three fields are deliberately null, and the manifest is NOT runner-ready
    //    until the owner fills them. They are not derivable here and guessing them
    //    would be the exact fraud this ceremony exists to prevent:
    //      * `owner_binary_digest` / `provider_executable_digest` — digests of the
    //        FROZEN candidate binary and provider executable. Those binaries do not
    //        exist yet (blocker 3 of G6-FORMAL-CEREMONY.md §8), and which build gets
    //        pinned is the owner's choice, not this process's.
    //      * `self_digest` — the domain digest over the other ten fields, so it can
    //        only be computed once they are final. The runner recomputes it and
    //        refuses any mismatch, which is what makes the pin independent.
    //    What IS proven by reaching this line: the production owner authority
    //    assembled from hardware-attested providers under a sealed ceremony.
    Ok(serde_json::json!({
        "schema": G6_AUTHORITY_ASSEMBLY_SCHEMA,
        "assembly_id": receipt.custody_floor,
        "provider_kind": "production",
        "production_authority_assembly": true,
        "owner_binary_digest": serde_json::Value::Null,
        "provider_executable_digest": serde_json::Value::Null,
        "owner_security_config_digest": owner_security_config_digest,
        "verification_key_registry": {
            "schema": "m1nd-verification-key-registry-v1",
            "registry_epoch": 1,
            "keys": { verification_key.key_id.clone(): verification_key },
        },
        "receipt_key_id": CUSTODY_SEALING_SEAT_KEY_ID,
        "max_future_clock_skew_ms": 0,
        "self_digest": serde_json::Value::Null,
    }))
}

/// Keychain custody handles for the ceremony's seats. Stable by construction: the
/// `kSecAttrLabel` a key is filed under IS its custody handle, so these strings
/// must never change without a re-provisioning ceremony.
#[cfg(target_os = "macos")]
const CUSTODY_KEYCHAIN_LABEL_PREFIX: &str = "world.m1nd.custody.g9";
#[cfg(target_os = "macos")]
const CUSTODY_SUBJECT_ID: &str = "m1nd-owner-authority";
#[cfg(target_os = "macos")]
const CUSTODY_SEALING_SEAT_KEY_ID: &str = "custody-sealing-seat-v1";
/// The owner's biometric seat (`owner_signature`). A distinct key id from every
/// voting seat, so the receipt's "the owner seat is never a voting seat" rule is
/// true by construction and not only by validation.
#[cfg(target_os = "macos")]
const CUSTODY_OWNER_BIOMETRIC_SEAT_KEY_ID: &str = "custody-owner-biometric-seat-v1";

// ===========================================================================
// The wiring battery, driven by the floor's OWN fake key store.
//
// `enclave_authority::test_support::MockEnclaveKeyStore` is the software P-256
// stand-in the floor's own tests already drive; it is REUSED here rather than
// re-created, so the door is proven against exactly the boundary the floor is.
//
// What these tests do NOT prove, and cannot: a real Secure Enclave key, the
// `KeychainAccessGroups` entitlement, the `kSecAccessControl` conformance check,
// and Touch ID. Those are the owner's hand and hardware and stay NOT_RUN
// (`docs/benchmarks/G9-CUSTODY-CEREMONY.md` §0); every test below runs against a
// temp directory and a software key, and none of them writes a ceremony an owner
// could mistake for theirs.
// ===========================================================================
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::Arc;

    use m1nd_control::autonomy::{
        IndependenceSpecCoreV1, IndependenceSpecV1, VerifierSeatV1, IMMUTABLE_FAILURE_DOMAINS,
        IMMUTABLE_QUORUM_THRESHOLD, INDEPENDENCE_SPEC_SCHEMA,
    };
    use tempfile::TempDir;

    use crate::enclave_authority::{
        test_support::MockEnclaveKeyStore, EnclaveAccessControlV1, SecureEnclaveKeyStoreV1,
        SECURE_ENCLAVE_CUSTODY_FLOOR_V1,
    };

    use super::*;

    const CONSTITUTION_DIGEST: &str =
        "1111111111111111111111111111111111111111111111111111111111111111";
    const SEAL_CLOCK_MS: u64 = 1_800_000_000_000;

    fn unattended_store() -> Arc<dyn SecureEnclaveKeyStoreV1> {
        Arc::new(MockEnclaveKeyStore::new(
            EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
        ))
    }

    fn biometric_store() -> Arc<dyn SecureEnclaveKeyStoreV1> {
        Arc::new(MockEnclaveKeyStore::new(
            EnclaveAccessControlV1::UserPresenceBiometricNonExportable,
        ))
    }

    /// A 0700 protected root, the owner's own prerequisite (P5). Created here
    /// because the ceremony refuses to create it — that is the point of P5.
    fn protected_root(temp: &TempDir) -> PathBuf {
        let root = temp.path().join("custody-root");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        root
    }

    /// The owner's independence spec: four voting seats over four distinct
    /// failure domains, sealed so its digest matches its own core.
    fn independence_spec() -> IndependenceSpecV1 {
        let voting_verifiers = [
            (
                "principal-alpha",
                "verifier-seat-alpha",
                "provider-a/model-a/runtime-a",
                "a",
            ),
            (
                "principal-bravo",
                "verifier-seat-bravo",
                "provider-b/model-b/runtime-b",
                "b",
            ),
            (
                "principal-charlie",
                "verifier-seat-charlie",
                "provider-c/model-c/runtime-c",
                "c",
            ),
            (
                "principal-delta",
                "verifier-seat-delta",
                "provider-d/model-d/runtime-d",
                "d",
            ),
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
        let mut spec = IndependenceSpecV1 {
            schema: INDEPENDENCE_SPEC_SCHEMA.to_owned(),
            core: IndependenceSpecCoreV1 {
                constitution_epoch: 1,
                voting_verifiers,
                quorum_threshold: IMMUTABLE_QUORUM_THRESHOLD,
                minimum_failure_domains: IMMUTABLE_FAILURE_DOMAINS,
                blind_isolation_policy_digest: "e".repeat(64),
                nonvoting_sentinel_id: "sentinel-0".to_owned(),
                proposer_executor_nonvoting: true,
                sentinel_nonvoting: true,
            },
            independence_spec_digest: String::new(),
        };
        spec.seal().unwrap();
        spec
    }

    fn staged_on_disk(root: &Path) -> StagedCeremonyV1 {
        let bytes = fs::read(root.join(CEREMONY_STAGING_FILE)).expect("the staging file exists");
        serde_json::from_slice(&bytes).expect("the staging file round-trips its own schema")
    }

    fn sealed_slot(root: &Path) -> PathBuf {
        root.join("custody-ceremony.sealed.json")
    }

    /// Drive Phase A and Phase B against the fake so Phase C has a complete
    /// ceremony to seal.
    fn stage_a_complete_ceremony(
        root: &Path,
        spec: &IndependenceSpecV1,
        unattended: &Arc<dyn SecureEnclaveKeyStoreV1>,
        biometric: &Arc<dyn SecureEnclaveKeyStoreV1>,
    ) {
        provision_seats_into_store(unattended.as_ref(), spec, root).expect("phase A stages");
        provision_owner_seat_into_store(biometric.as_ref(), root).expect("phase B stages");
    }

    // -----------------------------------------------------------------------
    // Phase A — provisioning stages what the constitution named, and nothing else
    // -----------------------------------------------------------------------

    /// Phase A mints one enclave key per seat the owner's independence spec
    /// names, and records the PUBLIC half plus the lineage digest. It invents no
    /// principal, no key id and no failure domain: every identity comes from the
    /// spec, which is what makes the later `bind_independence_spec` a real check
    /// instead of a tautology.
    #[test]
    fn provisioning_stages_every_seat_the_independence_spec_names() {
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let spec = independence_spec();
        let store = unattended_store();

        let staged = provision_seats_into_store(store.as_ref(), &spec, &root).unwrap();
        assert_eq!(staged.schema, CEREMONY_STAGING_SCHEMA);
        assert_eq!(staged.verifier_seats.len(), 4);
        assert!(
            staged.owner_biometric_seat_public_key.is_none(),
            "phase A must not claim the owner's seat"
        );

        for (seat, expected) in staged
            .verifier_seats
            .iter()
            .zip(&spec.core.voting_verifiers)
        {
            assert_eq!(seat.principal_id, expected.principal_id);
            assert_eq!(seat.key_id, expected.key_id);
            assert_eq!(seat.failure_domain, expected.failure_domain);
            assert_eq!(
                seat.bound_context_digest, spec.independence_spec_digest,
                "each permit is bound to the ceremony's own authority context, so a seat \
                 cannot be lifted from another ceremony's provisioning"
            );
            assert_eq!(seat.public_key.len(), 130, "65-byte uncompressed SEC1");
            assert!(seat.public_key.starts_with("04"));
            assert!(
                seat.public_key
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                "seat public keys are lowercase hex (G9-CUSTODY-CEREMONY.md §3)"
            );
        }

        let distinct: std::collections::BTreeSet<&str> = staged
            .verifier_seats
            .iter()
            .map(|seat| seat.public_key.as_str())
            .collect();
        assert_eq!(distinct.len(), 4, "each seat needs a DISTINCT enclave key");

        let sealing = staged.sealing_seat.as_ref().expect("the sealing seat");
        assert_eq!(sealing.key_id, CUSTODY_SEALING_SEAT_KEY_ID);
        assert!(
            !distinct.contains(sealing.public_key.as_str()),
            "the sealing seat is not one of the four voting seats"
        );

        assert_eq!(staged_on_disk(&root), staged);
        assert!(
            !sealed_slot(&root).exists(),
            "phase A seals nothing; only phase C may write the ceremony receipt"
        );
    }

    /// Re-running Phase A REFUSES. The floor's own law is never-open-or-create
    /// (`provision` fails closed on an existing label), so the door must not
    /// quietly re-stage over a ceremony that already minted keys.
    #[test]
    fn provisioning_refuses_a_second_run_rather_than_reprovisioning() {
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let spec = independence_spec();
        let store = unattended_store();

        let first = provision_seats_into_store(store.as_ref(), &spec, &root).unwrap();
        let refusal = provision_seats_into_store(store.as_ref(), &spec, &root)
            .expect_err("a second provisioning run must refuse");
        assert_eq!(refusal.code(), "custody_ceremony_seats_already_staged");
        assert_eq!(
            staged_on_disk(&root),
            first,
            "a refused re-run leaves the staged ceremony exactly as it was"
        );
    }

    /// A spec whose digest does not match its own core is refused BEFORE any key
    /// is minted. The digest is the lineage every seat is bound to, so a lying
    /// spec would bind four enclave keys to a context that does not exist.
    #[test]
    fn provisioning_refuses_a_spec_that_does_not_match_its_own_digest() {
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let mut spec = independence_spec();
        spec.independence_spec_digest = "f".repeat(64);
        let store = unattended_store();

        let refusal = provision_seats_into_store(store.as_ref(), &spec, &root)
            .expect_err("a spec that misstates its own digest must refuse");
        assert_eq!(refusal.code(), "custody_ceremony_incomplete");
        assert!(
            store.open("verifier-seat-alpha").is_err(),
            "nothing may be minted before the spec is accepted"
        );
        assert!(!root.join(CEREMONY_STAGING_FILE).exists());
    }

    /// The counts are law (`IMMUTABLE_VERIFIER_SEATS = 4`,
    /// `IMMUTABLE_FAILURE_DOMAINS = 3`), and they are checked before the first
    /// key is minted rather than after four are already in the keychain.
    #[test]
    fn provisioning_refuses_a_spec_that_breaks_the_immutable_counts() {
        let temp = TempDir::new().unwrap();
        let spec = independence_spec();

        let mut three_seats = spec.clone();
        three_seats.core.voting_verifiers.pop();
        three_seats.seal().unwrap();
        let root = protected_root(&temp);
        let store = unattended_store();
        assert_eq!(
            provision_seats_into_store(store.as_ref(), &three_seats, &root)
                .expect_err("three seats is not the frozen four")
                .code(),
            "custody_ceremony_incomplete"
        );
        assert!(store.open("verifier-seat-alpha").is_err());

        let mut two_domains = spec.clone();
        let shared = two_domains.core.voting_verifiers[0].failure_domain.clone();
        two_domains.core.voting_verifiers[2].failure_domain = shared.clone();
        two_domains.core.voting_verifiers[3].failure_domain = shared;
        two_domains.seal().unwrap();
        let other_temp = TempDir::new().unwrap();
        let other_root = protected_root(&other_temp);
        let other_store = unattended_store();
        assert_eq!(
            provision_seats_into_store(other_store.as_ref(), &two_domains, &other_root)
                .expect_err("two failure domains is below the frozen three")
                .code(),
            "custody_ceremony_incomplete"
        );
        assert!(other_store.open("verifier-seat-alpha").is_err());
    }

    /// The protected root is the owner's prerequisite, and the ceremony never
    /// creates or relaxes it: a group-readable root refuses before the keychain
    /// is touched.
    #[test]
    fn provisioning_refuses_a_protected_root_that_is_not_owner_only() {
        let temp = TempDir::new().unwrap();
        let loose = temp.path().join("loose-root");
        fs::create_dir(&loose).unwrap();
        fs::set_permissions(&loose, fs::Permissions::from_mode(0o755)).unwrap();
        let store = unattended_store();

        let refusal = provision_seats_into_store(store.as_ref(), &independence_spec(), &loose)
            .expect_err("a 0755 protected root must refuse");
        assert_eq!(refusal.code(), "custody_ceremony_protected_root_unusable");
        assert!(store.open("verifier-seat-alpha").is_err());
    }

    // -----------------------------------------------------------------------
    // Phase B — the owner's biometric seat
    // -----------------------------------------------------------------------

    /// Phase B has nothing to attach to until Phase A ran. It refuses, naming the
    /// verb that comes first.
    #[test]
    fn the_owner_seat_refuses_before_the_verifier_seats_exist() {
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let store = biometric_store();

        let refusal = provision_owner_seat_into_store(store.as_ref(), &root)
            .expect_err("phase B needs a staged phase A");
        assert_eq!(refusal.code(), "custody_ceremony_incomplete");
        assert!(
            refusal.to_string().contains("provision-seats"),
            "the refusal names the step the owner must run first: {refusal}"
        );
        assert!(store.open(CUSTODY_OWNER_BIOMETRIC_SEAT_KEY_ID).is_err());
    }

    /// The owner's seat is minted under the biometric class, carries the same
    /// ceremony lineage as the voting seats, and is never one of them.
    #[test]
    fn the_owner_seat_is_staged_under_its_own_key_and_never_votes() {
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let spec = independence_spec();
        let unattended = unattended_store();
        let biometric = biometric_store();

        provision_seats_into_store(unattended.as_ref(), &spec, &root).unwrap();
        let staged = provision_owner_seat_into_store(biometric.as_ref(), &root).unwrap();

        let owner_key = staged
            .owner_biometric_seat_public_key
            .as_deref()
            .expect("the owner seat is staged");
        assert_eq!(owner_key.len(), 130);
        assert!(owner_key.starts_with("04"));
        assert!(
            staged
                .verifier_seats
                .iter()
                .all(|seat| seat.public_key != owner_key),
            "owner_signature is never a voting quorum seat (G9-CUSTODY-CEREMONY.md §2 step 4)"
        );
        assert_eq!(
            staged.verifier_seats.len(),
            4,
            "phase B must not disturb the seats phase A staged"
        );
        assert_eq!(staged_on_disk(&root), staged);
        assert!(!sealed_slot(&root).exists());
    }

    /// Re-running Phase B refuses too: the owner's seat is minted once per
    /// ceremony, and a second mint would silently orphan the first key.
    #[test]
    fn the_owner_seat_refuses_a_second_run() {
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let spec = independence_spec();
        let unattended = unattended_store();
        let biometric = biometric_store();
        stage_a_complete_ceremony(&root, &spec, &unattended, &biometric);

        let refusal = provision_owner_seat_into_store(biometric.as_ref(), &root)
            .expect_err("the owner's seat is minted once");
        assert_eq!(refusal.code(), "custody_ceremony_seats_already_staged");
    }

    // -----------------------------------------------------------------------
    // Phase C — seal only over a COMPLETE ceremony
    // -----------------------------------------------------------------------

    /// Every incomplete shape refuses, each naming the exact missing piece, and
    /// none of them leaves a ceremony slot behind — a half-sealed custody root is
    /// worse than an unsealed one, because it looks finished.
    #[test]
    fn sealing_an_incomplete_ceremony_refuses_and_names_what_is_missing() {
        let spec = independence_spec();

        // 1. Nothing staged at all.
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let refusal = seal_with_store(
            unattended_store(),
            &spec,
            CONSTITUTION_DIGEST,
            &root,
            SEAL_CLOCK_MS,
        )
        .expect_err("an unstaged ceremony cannot seal");
        assert_eq!(refusal.code(), "custody_ceremony_incomplete");
        assert!(refusal.to_string().contains("provision-seats"));
        assert!(!sealed_slot(&root).exists());

        // 2. Phase A ran, phase B did not.
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let store = unattended_store();
        provision_seats_into_store(store.as_ref(), &spec, &root).unwrap();
        let refusal = seal_with_store(
            Arc::clone(&store),
            &spec,
            CONSTITUTION_DIGEST,
            &root,
            SEAL_CLOCK_MS,
        )
        .expect_err("a ceremony without the owner's seat cannot seal");
        assert_eq!(refusal.code(), "custody_ceremony_incomplete");
        assert!(
            refusal.to_string().contains("owner-seat"),
            "the refusal names the missing step: {refusal}"
        );
        assert!(!sealed_slot(&root).exists());

        // 3. A staged file that lost a seat.
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let store = unattended_store();
        let biometric = biometric_store();
        stage_a_complete_ceremony(&root, &spec, &store, &biometric);
        let mut staged = staged_on_disk(&root);
        staged.verifier_seats.pop();
        fs::write(
            root.join(CEREMONY_STAGING_FILE),
            serde_json::to_vec(&staged).unwrap(),
        )
        .unwrap();
        let refusal = seal_with_store(
            Arc::clone(&store),
            &spec,
            CONSTITUTION_DIGEST,
            &root,
            SEAL_CLOCK_MS,
        )
        .expect_err("three seats cannot seal");
        assert_eq!(refusal.code(), "custody_ceremony_incomplete");
        assert!(
            refusal.to_string().contains("found 3"),
            "the refusal counts what it found: {refusal}"
        );
        assert!(!sealed_slot(&root).exists());

        // 4. A staged file that lost its sealing seat.
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let store = unattended_store();
        let biometric = biometric_store();
        stage_a_complete_ceremony(&root, &spec, &store, &biometric);
        let mut staged = staged_on_disk(&root);
        staged.sealing_seat = None;
        fs::write(
            root.join(CEREMONY_STAGING_FILE),
            serde_json::to_vec(&staged).unwrap(),
        )
        .unwrap();
        let refusal = seal_with_store(store, &spec, CONSTITUTION_DIGEST, &root, SEAL_CLOCK_MS)
            .expect_err("no sealing seat, no seal");
        assert_eq!(refusal.code(), "custody_ceremony_incomplete");
        assert!(!sealed_slot(&root).exists());
    }

    /// The constitution digest is sealed into the receipt as a fact about the
    /// owner's constitution. A value that is not a lowercase sha-256 digest is
    /// refused rather than sealed — the receipt is the record, and a malformed
    /// record is a false one.
    #[test]
    fn sealing_refuses_a_constitution_digest_that_is_not_a_digest() {
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let spec = independence_spec();
        let store = unattended_store();
        let biometric = biometric_store();
        stage_a_complete_ceremony(&root, &spec, &store, &biometric);

        for candidate in ["", "not-a-digest", &"A".repeat(64), &"a".repeat(63)] {
            let refusal =
                seal_with_store(Arc::clone(&store), &spec, candidate, &root, SEAL_CLOCK_MS)
                    .expect_err("a malformed constitution digest must refuse");
            assert_eq!(refusal.code(), "custody_ceremony_receipt_invalid");
            assert!(!sealed_slot(&root).exists());
        }
    }

    /// The sealing key resolved from the keychain must be the one the ceremony
    /// provisioned. A key swapped under the label after Phase A would otherwise
    /// seal the ceremony in the name of a seat nobody minted.
    #[test]
    fn sealing_refuses_when_the_sealing_key_is_not_the_one_provisioned() {
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let spec = independence_spec();
        let store = unattended_store();
        let biometric = biometric_store();
        stage_a_complete_ceremony(&root, &spec, &store, &biometric);

        let mut staged = staged_on_disk(&root);
        let sealing = staged.sealing_seat.as_mut().unwrap();
        sealing.public_key = format!("04{}", "b".repeat(128));
        fs::write(
            root.join(CEREMONY_STAGING_FILE),
            serde_json::to_vec(&staged).unwrap(),
        )
        .unwrap();

        let refusal = seal_with_store(store, &spec, CONSTITUTION_DIGEST, &root, SEAL_CLOCK_MS)
            .expect_err("a swapped sealing key must refuse");
        assert_eq!(refusal.code(), "custody_ceremony_receipt_invalid");
        assert!(!sealed_slot(&root).exists());
    }

    /// A seat set that does not match the spec presented at seal time refuses.
    /// This is `bind_independence_spec` doing its job: the seats are sealed
    /// BEFORE any quorum vote is counted, so they must be the constitution's own.
    #[test]
    fn sealing_refuses_a_spec_whose_seats_are_not_the_provisioned_ones() {
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let spec = independence_spec();
        let store = unattended_store();
        let biometric = biometric_store();
        stage_a_complete_ceremony(&root, &spec, &store, &biometric);

        let mut other = spec.clone();
        other.core.voting_verifiers[0].key_id = "a-seat-nobody-minted".to_owned();
        other.seal().unwrap();
        let refusal = seal_with_store(store, &other, CONSTITUTION_DIGEST, &root, SEAL_CLOCK_MS)
            .expect_err("the sealed seats must be the spec's voting seats");
        assert_eq!(refusal.code(), "custody_ceremony_receipt_invalid");
        assert!(!sealed_slot(&root).exists());
    }

    /// The whole path: a complete ceremony seals, the receipt carries exactly
    /// what `G9-CUSTODY-CEREMONY.md` §3 names, the staging file is gone, and the
    /// receipt is read back through the SAME root-opening function `assemble`
    /// uses — so "assemble consumes what seal wrote" holds by construction.
    #[test]
    fn a_complete_ceremony_seals_and_the_assemble_seam_reads_it_back() {
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let spec = independence_spec();
        let store = unattended_store();
        let biometric = biometric_store();
        stage_a_complete_ceremony(&root, &spec, &store, &biometric);
        let staged = staged_on_disk(&root);

        let receipt = seal_with_store(
            Arc::clone(&store),
            &spec,
            CONSTITUTION_DIGEST,
            &root,
            SEAL_CLOCK_MS,
        )
        .expect("a complete ceremony seals");

        receipt.validate().expect("the sealed receipt validates");
        assert_eq!(receipt.custody_floor, SECURE_ENCLAVE_CUSTODY_FLOOR_V1);
        assert_eq!(
            receipt.independence_spec_digest,
            spec.independence_spec_digest
        );
        assert_eq!(receipt.constitution_digest, CONSTITUTION_DIGEST);
        assert_eq!(receipt.sealed_at, SEAL_CLOCK_MS);
        assert_eq!(
            receipt.attestation,
            crate::enclave_authority::CustodyAttestationDistinctionV1::secure_enclave_single_host(),
            "the receipt states what the enclave really provides and what it does not"
        );
        for (sealed, staged_seat) in receipt.verifier_seats.iter().zip(&staged.verifier_seats) {
            assert_eq!(sealed.principal_id, staged_seat.principal_id);
            assert_eq!(sealed.key_id, staged_seat.key_id);
            assert_eq!(sealed.failure_domain, staged_seat.failure_domain);
            assert_eq!(sealed.public_key, staged_seat.public_key);
            assert_eq!(
                sealed.bound_context_digest,
                staged_seat.bound_context_digest
            );
        }
        assert_eq!(
            Some(receipt.owner_biometric_seat_public_key.clone()),
            staged.owner_biometric_seat_public_key
        );
        receipt
            .bind_independence_spec(&spec)
            .expect("the sealed seats bind to the constitution's voting seats");

        assert!(sealed_slot(&root).exists(), "the sealed receipt landed");
        assert!(
            !root.join(CEREMONY_STAGING_FILE).exists(),
            "a completed ceremony leaves only the sealed receipt behind"
        );

        // THE SEAM: re-open the ceremony root exactly as `assemble` does and read
        // the receipt back.
        let (ceremony_root, _verification_key, _signer) =
            open_ceremony_root(store, &root, SEAL_CLOCK_MS).expect("assemble opens the same root");
        assert_eq!(
            ceremony_root.read_custody_ceremony().unwrap(),
            Some(receipt),
            "assemble reads back exactly the ceremony seal wrote"
        );
    }

    /// Sealing twice refuses: the ceremony's staging file is consumed by the
    /// first seal, so a second run has nothing complete to seal and says so.
    #[test]
    fn sealing_twice_refuses_rather_than_resealing() {
        let temp = TempDir::new().unwrap();
        let root = protected_root(&temp);
        let spec = independence_spec();
        let store = unattended_store();
        let biometric = biometric_store();
        stage_a_complete_ceremony(&root, &spec, &store, &biometric);
        seal_with_store(
            Arc::clone(&store),
            &spec,
            CONSTITUTION_DIGEST,
            &root,
            SEAL_CLOCK_MS,
        )
        .unwrap();

        let refusal = seal_with_store(store, &spec, CONSTITUTION_DIGEST, &root, SEAL_CLOCK_MS)
            .expect_err("a consumed ceremony cannot be re-sealed");
        assert_eq!(refusal.code(), "custody_ceremony_incomplete");
    }
}
