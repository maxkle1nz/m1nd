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
//! stubbing or dry-running any ceremony step. Accordingly this module PREPARES the
//! ceremony and never performs it. These remain NOT_RUN until the owner runs them
//! on an Apple Silicon / T2 Mac, present, with Touch ID enrolled, on a codesigned
//! binary carrying the `KeychainAccessGroups` entitlement:
//!
//! * Phase A steps 1-2 — provisioning the four unattended verifier seats.
//! * Phase A step 3 — the `kSecAccessControl` conformance check. The flag values
//!   are hand-rolled and `SecKeyCopyAttributes` does not read access control back,
//!   so the owner's live run is the only thing that proves them (§5 R5).
//! * Phase B step 4 — the owner's biometric seat. Touch ID has no stand-in.
//! * Phase C steps 5-7 — open, re-attest and seal against real enclave keys.
//! * Phase C step 8 — retiring the live proof key.
//!
//! There is deliberately no simulation path, and the battery asserts one is never
//! added (`m1nd-mcp/tests/custody_ceremony_wiring.rs`).

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
            Self::PlatformRefused(detail) => write!(formatter, "platform refused: {detail}"),
        }
    }
}

impl std::error::Error for CeremonyRefusalV1 {}

/// Admit or refuse a step on policy alone, before any platform call. Two
/// independent gates: the floor must exist on this target, and the biometric step
/// must have a human attached.
pub fn authorize_ceremony_step(
    verb: CustodyCeremonyVerbV1,
    attendance: CeremonyAttendanceV1,
) -> Result<(), CeremonyRefusalV1> {
    // Preflight is how an operator LEARNS the floor is unavailable here, so it
    // stays runnable on every target.
    if !cfg!(target_os = "macos") && verb != CustodyCeremonyVerbV1::Preflight {
        return Err(CeremonyRefusalV1::NotInstalledOnThisPlatform);
    }
    if verb.requires_owner_presence() && attendance == CeremonyAttendanceV1::Unattended {
        return Err(CeremonyRefusalV1::UnattendedOwnerPresenceRefused);
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

/// What Phase A and Phase B stage for Phase C to seal. Public key material and
/// lineage digests only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StagedCeremonyV1 {
    pub schema: String,
    /// The four unattended verifier seats, as `(principal, key_id, failure_domain,
    /// public_key, bound_context_digest)` rows.
    pub verifier_seats: Vec<serde_json::Value>,
    /// The owner's biometric seat public key, once Phase B has run.
    pub owner_biometric_seat_public_key: Option<String>,
}

/// Refuse to seal unless BOTH earlier phases completed. Reads only; a refusal here
/// leaves the protected root exactly as it was, because a half-sealed custody root
/// is worse than an unsealed one — it looks finished.
pub fn seal_requires_a_complete_ceremony(
    protected_root: &Path,
) -> Result<StagedCeremonyV1, CeremonyRefusalV1> {
    let path = protected_root.join(CEREMONY_STAGING_FILE);
    let bytes = fs::read(&path).map_err(|error| {
        CeremonyRefusalV1::CeremonyIncomplete(format!(
            "no staged ceremony at {}: run --custody-ceremony provision-seats first ({error})",
            path.display()
        ))
    })?;
    let staged: StagedCeremonyV1 = serde_json::from_slice(&bytes).map_err(|error| {
        CeremonyRefusalV1::CeremonyIncomplete(format!("staged ceremony is unreadable: {error}"))
    })?;
    if staged.verifier_seats.len() != 4 {
        return Err(CeremonyRefusalV1::CeremonyIncomplete(format!(
            "expected 4 staged verifier seats, found {}",
            staged.verifier_seats.len()
        )));
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

/// Inputs the `assemble` verb needs. Every path is owner-held: this binary holds
/// none of them and derives none of them, exactly as the G6 ceremony's script does.
#[derive(Clone, Debug)]
pub struct CeremonyRequestV1 {
    pub verb: CustodyCeremonyVerbV1,
    pub protected_root: std::path::PathBuf,
    pub owner_security_config: Option<std::path::PathBuf>,
    pub mission_config: Option<std::path::PathBuf>,
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
        CustodyCeremonyVerbV1::Seal => {
            match seal_requires_a_complete_ceremony(&request.protected_root) {
                Err(refusal) => (refusal.to_json(), 1),
                Ok(_staged) => (
                    owner_step_pending("seal", "Phase C steps 5-7 need real enclave keys"),
                    1,
                ),
            }
        }
        CustodyCeremonyVerbV1::ProvisionSeats => (
            owner_step_pending(
                "provision-seats",
                "Phase A steps 1-3 provision real Secure Enclave keys",
            ),
            1,
        ),
        CustodyCeremonyVerbV1::OwnerSeat => (
            owner_step_pending("owner-seat", "Phase B step 4 requires the owner's Touch ID"),
            1,
        ),
        CustodyCeremonyVerbV1::Assemble => assemble_verb(&request),
    }
}

/// The honest answer for a step whose provisioning half is the owner's hand. It
/// states what remains and does NOT claim the step ran.
fn owner_step_pending(verb: &str, why: &str) -> serde_json::Value {
    serde_json::json!({
        "schema": "m1nd-custody-ceremony-step-v1",
        "status": "NOT_RUN",
        "verb": verb,
        "detail": format!(
            "{why}. This step is the owner's, on an entitled binary at the owner's machine; \
             no agent may perform, simulate or dry-run it \
             (docs/benchmarks/G9-CUSTODY-CEREMONY.md §0)."
        ),
    })
}

#[cfg(not(target_os = "macos"))]
fn assemble_verb(_request: &CeremonyRequestV1) -> (serde_json::Value, i32) {
    (CeremonyRefusalV1::NotInstalledOnThisPlatform.to_json(), 1)
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

    use m1nd_control::AuthoritySigner;

    use crate::enclave_authority::{
        EnclaveAccessControlV1, EnclaveBackedWalRecordCrypto, EnclaveKeyAttestationV1,
        SealedProtectedRootV1, SecureEnclaveJournalHeadBackend,
        SecureEnclaveOwnerSecurityConfigRootBackend, SecureEnclaveProtectedEpochBackend,
        SecureEnclaveSigner, SecurityFrameworkEnclaveKeyStore,
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
    let root_check = protected_root_check(&request.protected_root);
    if root_check.state == "FAIL" {
        return Err(CeremonyRefusalV1::ProtectedRootUnusable(root_check.detail));
    }

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .map_err(|error| CeremonyRefusalV1::PlatformRefused(error.to_string()))?;

    // 1. Open the sealing seat and re-attest it. An unentitled binary cannot
    //    resolve the key at all, which is where P4 becomes visible.
    let key_store = Arc::new(SecurityFrameworkEnclaveKeyStore::new(
        CUSTODY_KEYCHAIN_LABEL_PREFIX,
        CUSTODY_SUBJECT_ID,
        EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
    ));
    let signer = SecureEnclaveSigner::open_attested(
        key_store,
        CUSTODY_SEALING_SEAT_KEY_ID,
        &EnclaveKeyAttestationV1::canonical(EnclaveAccessControlV1::PrivateKeyUsageNonExportable),
    )
    .map_err(|error| classify_provisioning_failure(&error.to_string()))?;
    let verification_key = signer.verification_key(now_ms, now_ms);
    let signer: Arc<dyn AuthoritySigner + Send + Sync> = Arc::new(signer);

    // 2. Open the protected roots. Two independent roots, as the assembly's own
    //    contract requires; the ceremony root is where the sealed receipt lives.
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

    // 3. The ceremony must already be sealed. No sealed receipt, no floor.
    let ceremony_root = SealedProtectedRootV1::open(
        &request.protected_root,
        &verification_key.public_key,
        Arc::clone(&signer),
        verification_key.clone(),
    )
    .map_err(|error| CeremonyRefusalV1::ProtectedRootUnusable(error.to_string()))?;
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
