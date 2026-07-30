//! Acceptance battery for the G9 Secure Enclave custody ceremony WIRING.
//!
//! Written from the step list in `docs/benchmarks/G9-CUSTODY-CEREMONY.md` §2 and
//! the measured gap in its §4, plus the G6 blocker named in
//! `docs/benchmarks/G6-FORMAL-CEREMONY.md` §8 item 2 and the next step named in
//! `docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md` §7.
//!
//! # What this battery does and does not prove
//!
//! It proves the DOOR: that the custody floor is reachable from a non-test path,
//! that the door is a human-origin CLI ingress no transport can reach, that the
//! owner-presence step fails closed when unattended, and that a partial ceremony
//! commits nothing. It proves these against the software boundary — verb policy,
//! refusal classification, protected-root preflight, and source-level reachability.
//!
//! It does NOT prove the ceremony. Provisioning a Secure Enclave key, satisfying
//! Touch ID, and persisting into the data-protection keychain on a codesigned,
//! entitled binary are the owner's hand and hardware — `G9-CUSTODY-CEREMONY.md` §0
//! prohibits an agent from performing, simulating or faking any of it. Every step
//! that needs the owner is marked NOT_RUN in its own doc comment below rather than
//! covered by a fixture that would be a lie. This is the house style the lifecycle
//! gate and SPEC-1 already use.

use std::fs;
use std::path::{Path, PathBuf};

use m1nd_mcp::custody_ceremony::{
    authorize_ceremony_step, classify_provisioning_failure, preflight, CeremonyAttendanceV1,
    CustodyCeremonyVerbV1, CUSTODY_CEREMONY_VERBS, G6_AUTHORITY_ASSEMBLY_MANIFEST_FIELDS,
    G6_AUTHORITY_ASSEMBLY_SCHEMA,
};

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is <repo>/m1nd-mcp.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("m1nd-mcp has a parent")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Strip the trailing `#[cfg(test)]` module so a "production caller" search cannot
/// be satisfied by a test.
///
/// Anchored to a `#[cfg(test)]` at COLUMN ZERO, which is how 80 of this crate's 82
/// files declare their test module. The looser `str::find` is wrong in a way that
/// silently disarms the search: a doc comment merely MENTIONING `#[cfg(test)]`
/// truncates the whole file, and the production code below it is never scanned.
/// That is exactly how this helper first failed, on the one file whose module doc
/// quotes the measurement it closes.
fn without_test_modules(source: &str) -> String {
    let marker = "\n#[cfg(test)]\n";
    match source.find(marker) {
        Some(index) => source[..index].to_owned(),
        None => source.to_owned(),
    }
}

// ===========================================================================
// 1. The verb surface — a closed set, parsed one way
// ===========================================================================

/// The ceremony admits exactly the staged step list and nothing else. A closed
/// set is what lets the refusal below be exhaustive.
#[test]
fn the_ceremony_verb_set_is_closed() {
    assert_eq!(
        CUSTODY_CEREMONY_VERBS,
        &[
            "preflight",
            "provision-seats",
            "owner-seat",
            "seal",
            "assemble"
        ],
        "the verb set is the ceremony's step list (G9-CUSTODY-CEREMONY.md §2)"
    );
    for verb in CUSTODY_CEREMONY_VERBS {
        let parsed: CustodyCeremonyVerbV1 = verb.parse().expect("staged verb parses");
        assert_eq!(&parsed.as_str(), verb, "verb round-trips through its token");
    }
}

/// An unknown verb refuses fail-closed instead of defaulting to a step. There is
/// no default: `--medulla-migrate` set that precedent and this ceremony is more
/// dangerous than a storage migration.
#[test]
fn an_unknown_verb_refuses_rather_than_defaulting() {
    for candidate in [
        "",
        "provision",
        "run",
        "PREFLIGHT",
        "assemble ",
        "--assemble",
    ] {
        let parsed = candidate.parse::<CustodyCeremonyVerbV1>();
        assert_eq!(
            parsed.err().map(|refusal| refusal.code()),
            Some("custody_ceremony_unknown_verb"),
            "'{candidate}' must refuse, never resolve to a step"
        );
    }
}

/// Exactly one step needs the owner's body, and exactly the steps that write
/// custody state say so. The policy function below keys off these.
#[test]
fn only_the_owner_seat_step_claims_owner_presence() {
    let presence: Vec<&str> = CUSTODY_CEREMONY_VERBS
        .iter()
        .filter(|verb| {
            verb.parse::<CustodyCeremonyVerbV1>()
                .unwrap()
                .requires_owner_presence()
        })
        .copied()
        .collect();
    assert_eq!(
        presence,
        vec!["owner-seat"],
        "the biometric seat is the only irreducibly human step (§2 phase B)"
    );

    let mutating: Vec<&str> = CUSTODY_CEREMONY_VERBS
        .iter()
        .filter(|verb| {
            verb.parse::<CustodyCeremonyVerbV1>()
                .unwrap()
                .mutates_custody()
        })
        .copied()
        .collect();
    assert_eq!(
        mutating,
        vec!["provision-seats", "owner-seat", "seal"],
        "preflight and assemble must never mint or seal custody material"
    );
}

// ===========================================================================
// 2. The negative fixtures that matter most
// ===========================================================================

/// **A transport-originated attempt refuses.** The ceremony must be unreachable
/// from MCP. This is the `--birth` precedent's core property: the CLI ingress IS
/// the human-origin fact, so no tool, header or payload field can stand in for it.
/// Proven against the real registry — one accidental `#[tool]` would fail here.
#[test]
fn the_ceremony_is_absent_from_the_mcp_tool_registry() {
    let registry = m1nd_mcp::server::all_tool_schemas();
    let tools = registry["tools"].as_array().expect("tools array");
    assert!(!tools.is_empty(), "registry must not be empty");

    for tool in tools {
        let name = tool["name"].as_str().unwrap_or("<unnamed>");
        let rendered = tool.to_string().to_lowercase();
        for forbidden in [
            "custody_ceremony",
            "custody-ceremony",
            "provision_agent_enclave_seat",
            "owner_biometric_seat",
            "seal_custody_ceremony",
        ] {
            assert!(
                !rendered.contains(forbidden),
                "tool '{name}' exposes ceremony surface '{forbidden}' over MCP; the ceremony \
                 is CLI-ingress only (G9-CUSTODY-CEREMONY.md §0, §4)"
            );
        }
    }
}

/// **An agent-simulated origin refuses.** The stamp is a value the owner
/// constructs about ITSELF, never data that arrives — so it has no `Deserialize`,
/// no `FromStr`, no `Default`, and exactly one construction site. Held as a
/// source-level guard because that is the only place the claim is checkable: a
/// second construction site anywhere in the crate would silently widen the door.
#[test]
fn the_ingress_stamp_has_exactly_one_construction_site() {
    let root = repo_root();
    let mut sites: Vec<String> = Vec::new();
    let src = root.join("m1nd-mcp").join("src");
    let mut stack = vec![src.clone()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            let source = read(&path);
            // The definition itself lives in custody_ceremony.rs; every OTHER
            // occurrence is a call site and must be the CLI dispatch.
            let is_definition_file =
                path.file_name().and_then(|name| name.to_str()) == Some("custody_ceremony.rs");
            if is_definition_file {
                continue;
            }
            if source.contains("from_cli_ingress") {
                sites.push(
                    path.strip_prefix(&root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    sites.sort();
    assert_eq!(
        sites,
        vec!["m1nd-mcp/src/main.rs".to_owned()],
        "the ingress stamp must be constructed ONLY by the CLI dispatch; found {sites:?}"
    );

    // And it must not be reachable from serde or a string.
    let module = read(&src.join("custody_ceremony.rs"));
    let stamp_region = module
        .split("pub struct OwnerCeremonyIngressV1")
        .nth(1)
        .expect("the stamp type exists");
    let stamp_impl = stamp_region
        .split("\n// ===")
        .next()
        .expect("stamp section is delimited");
    for forbidden in ["Deserialize", "FromStr", "Default", "serde"] {
        assert!(
            !stamp_impl.contains(forbidden),
            "OwnerCeremonyIngressV1 must not derive or implement {forbidden}: a wire payload \
             could then manufacture the human-origin stamp"
        );
    }
}

/// **The owner-presence step fails closed when unattended.** An unattended
/// process must refuse BEFORE reaching the enclave, rather than blocking forever
/// on a Touch ID prompt nobody is there to answer.
///
/// The honest limit, stated where the code is: this software check is not the
/// security boundary. The real gate is `kSecAccessControlUserPresence` enforced by
/// the Secure Enclave at signing time (`enclave_authority.rs` access_control_flags).
/// This refusal only stops a scheduler from hanging on a prompt.
#[test]
fn the_owner_seat_step_refuses_when_unattended() {
    let refusal = authorize_ceremony_step(
        CustodyCeremonyVerbV1::OwnerSeat,
        CeremonyAttendanceV1::Unattended,
    )
    .expect_err("the biometric seat must refuse an unattended process");
    assert_eq!(
        refusal.code(),
        "custody_ceremony_unattended_presence_refused"
    );

    // Every other step is allowed to run unattended (subject to platform), so the
    // refusal is targeted rather than a blanket lock.
    for verb in ["preflight", "provision-seats", "seal", "assemble"] {
        let parsed: CustodyCeremonyVerbV1 = verb.parse().unwrap();
        let outcome = authorize_ceremony_step(parsed, CeremonyAttendanceV1::Unattended);
        assert_ne!(
            outcome.err().map(|refusal| refusal.code()),
            Some("custody_ceremony_unattended_presence_refused"),
            "'{verb}' does not need the owner's body and must not claim to"
        );
    }
}

/// **An unentitled or unsigned binary fails closed with the honest error.**
/// Secure Enclave keys are only permanent in the data-protection keychain, which
/// an unentitled binary can neither write nor resolve — so provision, open and
/// sign all fail. This is the state of EVERY build the project ships: the released
/// binary is signed without the entitlement, because a raw executable that claims
/// this restricted entitlement is SIGKILLed by AMFI at launch (measured 2026-07-30,
/// `build/README.md`), and a locally-built binary carries no signature at all. So
/// the failure must NAME that cause instead of surfacing a raw OSStatus.
///
/// Tested at the classifier, not by provisioning: producing the real failure means
/// touching the enclave, which §0 prohibits.
#[test]
fn an_unentitled_binary_fails_closed_with_the_honest_error() {
    // errSecMissingEntitlement == -34018; errSecInteractionNotAllowed == -25308.
    for platform_error in [
        "SecKeyCreateRandomKey failed: -34018",
        "error -34018: A required entitlement isn't present",
        "errSecMissingEntitlement",
    ] {
        assert_eq!(
            classify_provisioning_failure(platform_error).code(),
            "custody_ceremony_keychain_entitlement_missing",
            "'{platform_error}' must be reported as the entitlement prerequisite (P4), \
             not as an opaque platform error"
        );
    }

    // A genuinely different failure must NOT be laundered into the entitlement
    // story — a wrong diagnosis would send the owner to re-sign a fine binary.
    let other = classify_provisioning_failure("SecKeyCreateSignature failed: -25300");
    assert_eq!(other.code(), "custody_ceremony_platform_refused");
}

// ===========================================================================
// 3. Preflight — the one step that is safe to run, and provisions nothing
// ===========================================================================

/// Preflight reports every prerequisite and creates NOTHING. This is the step an
/// agent may run, so its non-mutation is load-bearing, not cosmetic.
#[test]
fn preflight_reports_every_prerequisite_and_writes_nothing() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("custody-root");
    fs::create_dir(&root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    }

    let report = preflight(&root);
    let ids: Vec<String> = report.checks.iter().map(|check| check.id.clone()).collect();
    assert_eq!(
        ids,
        vec!["P1", "P2", "P3", "P4", "P5", "P6", "P7", "P8"],
        "preflight reports the full prerequisite table (G9-CUSTODY-CEREMONY.md §1)"
    );

    // Nothing was minted. The protected root is exactly as empty as it was.
    let entries: Vec<_> = fs::read_dir(&root).unwrap().collect();
    assert!(
        entries.is_empty(),
        "preflight must not create any file in the protected root"
    );

    let json = report.to_json();
    assert_eq!(json["schema"], "m1nd-custody-ceremony-preflight-v1");
    assert!(
        json["checks"].is_array(),
        "preflight prints one closed JSON object, like every other one-shot mode"
    );
}

/// A protected root that is not `0700` refuses. The sealed slots' anti-rollback is
/// filesystem strength (the ceremony receipt says so in its own non-claims), so a
/// group- or world-readable root is not a warning, it is a refusal.
#[cfg(unix)]
#[test]
fn preflight_refuses_a_protected_root_that_is_not_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("loose-root");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();

    let report = preflight(&root);
    let p5 = report
        .checks
        .iter()
        .find(|check| check.id == "P5")
        .expect("P5 is the protected root check");
    assert_eq!(p5.state, "FAIL", "0755 protected root must fail P5");
    assert!(
        !report.ready,
        "a failing prerequisite makes preflight not ready"
    );
}

/// A missing protected root is reported, not created. Preflight never provisions
/// its own prerequisite — that would hide the owner's own step from them.
#[test]
fn preflight_reports_a_missing_protected_root_without_creating_it() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("absent-root");

    let report = preflight(&root);
    let p5 = report.checks.iter().find(|check| check.id == "P5").unwrap();
    assert_eq!(p5.state, "FAIL");
    assert!(
        !root.exists(),
        "preflight must not create the protected root it is checking"
    );
}

// ===========================================================================
// 4. Platform floor — absent by construction is a REFUSAL, never a fallback
// ===========================================================================

/// On a non-macOS target the module is absent by construction, so every step that
/// touches custody refuses NOT_INSTALLED. The one thing it must never do is fall
/// back to software assurance — that is the whole point of the Path-B floor.
#[cfg(not(target_os = "macos"))]
#[test]
fn every_custody_step_is_not_installed_off_macos() {
    for verb in ["provision-seats", "owner-seat", "seal", "assemble"] {
        let parsed: CustodyCeremonyVerbV1 = verb.parse().unwrap();
        let refusal = authorize_ceremony_step(parsed, CeremonyAttendanceV1::InteractiveTerminal)
            .expect_err("custody steps are unavailable off macOS");
        assert_eq!(
            refusal.code(),
            "custody_ceremony_not_installed",
            "'{verb}' must refuse off macOS instead of selecting a software fallback"
        );
    }
    // Preflight still runs everywhere — it is how an operator LEARNS the floor is
    // unavailable here.
    assert!(authorize_ceremony_step(
        CustodyCeremonyVerbV1::Preflight,
        CeremonyAttendanceV1::InteractiveTerminal
    )
    .is_ok());
}

/// **Presence is asked before platform, on every target.** Who invoked the
/// ceremony is a fact about the CALLER; whether the floor is compiled in is a
/// fact about the HOST. An unattended process claiming the owner's biometric seat
/// makes a false claim on Linux and Windows exactly as it does on macOS, so the
/// weaker "not installed here" answer — which reads as *this would have proceeded
/// on a Mac* — must never be the one reported.
#[cfg(not(target_os = "macos"))]
#[test]
fn the_presence_refusal_outranks_the_platform_refusal_off_macos() {
    let refusal = authorize_ceremony_step(
        CustodyCeremonyVerbV1::OwnerSeat,
        CeremonyAttendanceV1::Unattended,
    )
    .expect_err("an unattended owner-seat attempt refuses everywhere");
    assert_eq!(
        refusal.code(),
        "custody_ceremony_unattended_presence_refused",
        "the presence gate is platform-independent and is asked FIRST"
    );
}

/// On macOS the steps are admitted by policy (the hardware, entitlement and the
/// owner's hand are still required, and are checked at their own layers).
#[cfg(target_os = "macos")]
#[test]
fn custody_steps_are_admitted_by_policy_on_macos() {
    for verb in ["preflight", "provision-seats", "seal", "assemble"] {
        let parsed: CustodyCeremonyVerbV1 = verb.parse().unwrap();
        assert!(
            authorize_ceremony_step(parsed, CeremonyAttendanceV1::InteractiveTerminal).is_ok(),
            "'{verb}' is admitted by policy on macOS"
        );
    }
    assert!(authorize_ceremony_step(
        CustodyCeremonyVerbV1::OwnerSeat,
        CeremonyAttendanceV1::InteractiveTerminal
    )
    .is_ok());
}

// ===========================================================================
// 5. The seam itself — the floor is reachable from a NON-test path
// ===========================================================================

/// **The measured gap, closed.** `G9-CUSTODY-CEREMONY.md` §4 measured
/// `assemble_production_owner_authority_v1` as having three callers, ALL inside
/// `#[cfg(test)]`. This test fails if that is still true.
#[test]
fn the_production_authority_assembly_has_a_non_test_caller() {
    let root = repo_root();
    let src = root.join("m1nd-mcp").join("src");
    let mut production_callers: Vec<String> = Vec::new();

    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            let production = without_test_modules(&read(&path));
            // The definition site is not a caller.
            let calls = production
                .match_indices("assemble_production_owner_authority_v1(")
                .count();
            let definitions = production
                .matches("pub fn assemble_production_owner_authority_v1(")
                .count();
            if calls > definitions {
                production_callers.push(
                    path.strip_prefix(&root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    assert!(
        !production_callers.is_empty(),
        "the custody floor is still an unwired island: no non-test code calls \
         assemble_production_owner_authority_v1 (G9-CUSTODY-CEREMONY.md §4, decision §7)"
    );
    assert!(
        production_callers
            .iter()
            .any(|path| path.ends_with("custody_ceremony.rs")),
        "the assemble verb is the intended production caller; found {production_callers:?}"
    );
}

/// The G6 formal run refuses without "a pinned production authority assembly"
/// (`G6-FORMAL-CEREMONY.md` §8 item 2). The manifest this ceremony emits must
/// match the runner's contract EXACTLY — the runner requires an exact field set
/// and recomputes the self digest, so a near-miss is a hard refusal at run time.
///
/// The field set and schema are pinned here against
/// `scripts/benchmark/m1nd10_g6_blind_runner.py` so a drift on either side is
/// caught in CI rather than at the owner's one sealed run.
#[test]
fn the_emitted_authority_assembly_matches_the_g6_runner_contract() {
    assert_eq!(
        G6_AUTHORITY_ASSEMBLY_SCHEMA,
        "m1nd10-g6-authority-assembly-v1"
    );

    let mut fields: Vec<&str> = G6_AUTHORITY_ASSEMBLY_MANIFEST_FIELDS.to_vec();
    fields.sort_unstable();
    let mut expected = vec![
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
    expected.sort_unstable();
    assert_eq!(
        fields, expected,
        "the emitted manifest must carry the runner's exact field set \
         (AUTHORITY_ASSEMBLY_FIELDS in m1nd10_g6_blind_runner.py)"
    );

    // The runner also pins the field set on its side. Read it and prove the two
    // never drift apart silently.
    let runner = read(
        &repo_root()
            .join("scripts")
            .join("benchmark")
            .join("m1nd10_g6_blind_runner.py"),
    );
    for field in G6_AUTHORITY_ASSEMBLY_MANIFEST_FIELDS {
        assert!(
            runner.contains(&format!("\"{field}\"")),
            "field '{field}' is not present in the G6 runner's contract"
        );
    }
    assert!(
        runner.contains("AUTHORITY_ASSEMBLY_SCHEMA = \"m1nd10-g6-authority-assembly-v1\""),
        "the runner's schema constant moved; the emitted manifest would be refused"
    );
}

// ===========================================================================
// 6. Partial ceremony commits nothing
// ===========================================================================

/// A ceremony that stops halfway leaves NOTHING half-committed. The seal step is
/// validate-then-write: an invalid receipt must not leave a slot behind, because a
/// half-sealed custody root is worse than an unsealed one — it looks done.
///
/// Proven at the boundary an agent may touch: the seal verb refuses before it
/// opens the protected root when the ceremony inputs are incomplete, so no slot
/// file is created.
#[test]
fn a_partial_ceremony_leaves_nothing_behind() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("custody-root");
    fs::create_dir(&root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    }

    // Sealing with no provisioned seats is exactly the "stopped halfway" case.
    let refusal = m1nd_mcp::custody_ceremony::seal_requires_a_complete_ceremony(&root)
        .expect_err("sealing an incomplete ceremony must refuse");
    assert_eq!(refusal.code(), "custody_ceremony_incomplete");

    let entries: Vec<_> = fs::read_dir(&root).unwrap().collect();
    assert!(
        entries.is_empty(),
        "a refused seal must leave the protected root untouched; found {} entries",
        entries.len()
    );
    assert!(
        !root.join("custody-ceremony.sealed.json").exists(),
        "no ceremony slot may exist without a complete ceremony"
    );
}

// ===========================================================================
// 7. What remains NOT_RUN until the owner's hand
// ===========================================================================

/// NOT_RUN, and not fakeable. These steps need the owner physically present at an
/// Apple Silicon / T2 Mac with Touch ID enrolled, running a codesigned binary that
/// carries the `KeychainAccessGroups` entitlement:
///
/// * Phase A step 1-2 — provisioning the four unattended verifier seats into the
///   data-protection keychain, and reading their real `SecKeyCopyAttributes` back.
/// * Phase A step 3 — the `kSecAccessControl` conformance check. The flag values
///   (`1 << 30`, `1 << 0`) are hand-rolled and `SecKeyCopyAttributes` does not read
///   access control back, so the owner's live run is the ONLY thing that proves
///   them (G9-CUSTODY-CEREMONY.md §5 R5).
/// * Phase B step 4 — the owner's biometric seat. Touch ID cannot be stood in for.
/// * Phase C steps 5-7 — open/re-attest/seal against real enclave keys.
/// * Phase C step 8 — retiring the live proof key.
///
/// This test asserts only that the battery does not silently claim otherwise: the
/// module must declare these as NOT_RUN rather than shipping a fixture that
/// imitates them.
#[test]
fn the_owner_only_steps_are_declared_not_run_rather_than_faked() {
    let module = read(&repo_root().join("m1nd-mcp/src/custody_ceremony.rs"));
    assert!(
        module.contains("NOT_RUN"),
        "the module must declare which steps remain NOT_RUN until the owner runs them"
    );
    // And it must not carry a simulation escape hatch.
    for forbidden in [
        "fn simulate_ceremony",
        "fn fake_ceremony",
        "dry_run_provision",
        "mock_owner_seat",
    ] {
        assert!(
            !module.contains(forbidden),
            "the ceremony must have no simulation path ({forbidden}); G9-CUSTODY-CEREMONY.md §0"
        );
    }
}

// ===========================================================================
// 8. The wiring gap itself — the measured zeros must become non-zeros
// ===========================================================================

/// Count the non-test references to `symbol` outside its own definition file.
/// This is the same measurement `G9-CUSTODY-CEREMONY.md` §4 published as a table
/// of zeros, run as a test so the table can never quietly go stale again.
fn production_references_outside_the_floor(symbol: &str) -> Vec<String> {
    let root = repo_root();
    let src = root.join("m1nd-mcp").join("src");
    let mut referencing = Vec::new();
    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) == Some("enclave_authority.rs") {
                continue;
            }
            if without_test_modules(&read(&path)).contains(symbol) {
                referencing.push(
                    path.strip_prefix(&root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    referencing.sort();
    referencing
}

/// **The island is connected.** `G9-CUSTODY-CEREMONY.md` §4 measured every one of
/// the floor's provisioning and sealing entry points at ZERO references outside
/// its own file: the custody floor was "a fully-implemented, fully-tested,
/// entirely unwired island". Wiring the three owner verbs is what makes these
/// non-zero, and the door is the only thing allowed to reference them.
#[test]
fn the_ceremony_verbs_reference_the_floor_s_provisioning_and_sealing_primitives() {
    for symbol in [
        "provision_agent_enclave_seat",
        "provision_owner_biometric_seat",
        "seal_custody_ceremony",
        "bind_independence_spec",
    ] {
        let referencing = production_references_outside_the_floor(symbol);
        assert!(
            !referencing.is_empty(),
            "'{symbol}' still has zero non-test references outside the floor — the custody \
             floor is an unwired island (G9-CUSTODY-CEREMONY.md §4)"
        );
        assert_eq!(
            referencing,
            vec!["m1nd-mcp/src/custody_ceremony.rs".to_owned()],
            "only the ceremony door may drive the floor's provisioning and sealing \
             primitives; found {referencing:?}"
        );
    }
}

/// **No verb answers with a placeholder.** While the enclave wiring was absent,
/// `provision-seats`, `owner-seat` and `seal` returned a hand-written `NOT_RUN`
/// object from a single helper. Once they are wired, that helper must be GONE —
/// a step that reports `NOT_RUN` without asking the platform is indistinguishable
/// from a step that asked and was refused, and only one of those is honest.
///
/// The steps that genuinely cannot run here now say so through the real refusal
/// path: an unentitled binary cannot persist or resolve an enclave key, and the
/// classifier names that prerequisite (P4) rather than an opaque OSStatus.
#[test]
fn no_ceremony_verb_answers_with_a_hand_written_placeholder() {
    let module = read(&repo_root().join("m1nd-mcp/src/custody_ceremony.rs"));
    assert!(
        !module.contains("fn owner_step_pending"),
        "the placeholder answer must be deleted once the verbs reach the floor, not kept \
         beside it (G9-CUSTODY-CEREMONY.md §4, §5 R2)"
    );
}

/// The owner's biometric seat and the agent's verifier seats are minted by two
/// different functions that refuse each other's access-control class. Neither can
/// stand in for the other, so no agent path can mint the human seat even if the
/// door were reached — which is the floor's own `HumanSeatProvisioningRefused`
/// rule extended to its mirror (`G9-CUSTODY-CEREMONY.md` §2 step 4: the owner
/// seat "needs a *separate, owner-only* entry point").
#[test]
fn the_owner_seat_has_its_own_entry_point_and_the_two_refuse_each_other() {
    let floor = read(&repo_root().join("m1nd-mcp/src/enclave_authority.rs"));
    let production = without_test_modules(&floor);
    assert!(
        production.contains("pub fn provision_owner_biometric_seat("),
        "the owner-only provisioning entry point must exist beside the agent one"
    );
    assert!(
        production.contains("HumanSeatProvisioningRefused"),
        "the agent entry point keeps refusing the human seat fail-closed"
    );
}
