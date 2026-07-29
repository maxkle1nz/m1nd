//! SPEC-2 — `brain.bootstrap.birth`, the birth ceremony's server verb.
//!
//! The normative document is `docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §2
//! (RATIFIED, owner, 2026-07-29, all four §6 items — item 4 being the `human-cli`
//! allowlist entry this module exists to honour).
//!
//! **What is here in the BATTERY commit and what is not.** This file lands with
//! the acceptance battery so the asserts have a seam to name, exactly as SPEC-1's
//! battery landed `SessionState::explicit_brain_selector`. It is born REFUSING:
//! [`run_birth`] answers `birth_not_implemented`, which is today's literal truth
//! — the ceremony does not exist, `m1nd init` is `installSkills`. The battery is
//! therefore RED for its own reason and not for a missing symbol.
//!
//! **The one thing that is NOT a stub, because it is the spec's substance:** the
//! origin type. `HumanOrigin` is a closed Rust enum, so the closed server-side
//! allowlist §2 requires is the TYPE SYSTEM rather than a string check — and a
//! client-claimed origin string cannot become one, at any seam, ever. There is no
//! `FromStr`, no `Deserialize`, and no public constructor: the only way to obtain
//! a value is [`HumanOrigin::stamp_ceremony_cli`], which the owner's own ceremony
//! ingress calls about ITSELF.

use serde_json::json;

/// The CLOSED set of HUMAN origins the birth verb admits
/// (`docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §2, owner-ratified §6 item 4).
///
/// A Rust enum rather than a string allow-list on purpose. `receipt_import`'s
/// gate (`system_blocks_handlers.rs`) validates a CLIENT-SUPPLIED string against
/// a const array, and its own comment states the honest limit: the token is
/// forgeable, so it closes the cheap vector only. SPEC-2 is one floor higher —
/// `PositiveSovereign` — and its ratify counter-precedent
/// (`system_blocks_handlers.rs:435`) is explicit that "a client-supplied origin
/// token (including 'human-ui') grants no authority". So the origin here is not
/// data that arrives; it is a value the OWNER constructs about itself. No
/// `Deserialize`, no `FromStr`, no `pub fn new`. A params field named
/// `birth_via`, `origin`, or anything else is not read by any code path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanOrigin {
    /// The owner's own Human View screen (`human-ui`). Named by the ratified
    /// allowlist; no stamping seam is installed in this PR.
    Ui,
    /// The h4nd tray's native prompt behind Touch ID (`human-touchid`). Named by
    /// the ratified allowlist; no stamping seam is installed in this PR.
    TouchId,
    /// The P2 ceremony (`human-cli`) — `m1nd init --birth <root>`, which reaches
    /// the owner as its own `--birth` ingress. The ONLY stamp installed today.
    Cli,
}

impl HumanOrigin {
    /// The wire token, for receipts and refusals.
    pub fn as_str(self) -> &'static str {
        match self {
            HumanOrigin::Ui => "human-ui",
            HumanOrigin::TouchId => "human-touchid",
            HumanOrigin::Cli => "human-cli",
        }
    }
}

/// The ratified allowlist as tokens, for payloads that must NAME the closed set
/// (the refusal's `allowed_origins`, mirroring `receipt_import`'s shape).
pub const BIRTH_HUMAN_ORIGINS: &[&str] = &["human-ui", "human-touchid", "human-cli"];

/// The origins that have a STAMPING SEAM in this binary today.
///
/// The allowlist above is what the owner ratified; this is what the owner can
/// actually stamp. `receipt_import`'s const carries the same discipline in prose
/// ("the remaining native gestures join this list in LATER steps, and only WHEN
/// their components exist"); stating it as data lets a test hold the line.
pub const BIRTH_ORIGINS_WITH_A_STAMPING_SEAM: &[&str] = &["human-cli"];

/// The response schema every birth answer wears — receipt and refusal alike, so
/// both transports emit one shape and an agent can branch on `refused` without
/// parsing prose (SPEC-1's `m1nd-graph-ingest-refresh-v1` precedent).
pub const BIRTH_SCHEMA: &str = "m1nd-brain-birth-v1";

/// The semantic action, as the M1ND-10 catalog names it.
pub const BIRTH_ACTION: &str = "brain.bootstrap.birth";

/// What the ceremony asks for. Deliberately NOT `Deserialize`: this struct is
/// built by the owner's ceremony ingress from its own argv, never parsed from a
/// client payload.
#[derive(Clone, Debug)]
pub struct BirthRequest {
    /// The destination repo root, as the human typed it.
    pub root: String,
    /// Who is recorded as having driven the ceremony.
    pub agent_id: String,
    /// Present ONLY so the sovereign path can refuse it (cp32 requirement:
    /// forbid `allow_overlap:true` off the sovereign path — and the sovereign
    /// path forbids it too, which is the stronger reading §2 states as "no
    /// `allow_overlap` below sovereign").
    pub allow_overlap: bool,
    /// Passed through to the first ingest.
    pub include_dotfiles: bool,
}

impl BirthRequest {
    /// The ceremony's own request, from the root the human named.
    pub fn ceremony(root: impl Into<String>, agent_id: impl Into<String>) -> Self {
        BirthRequest {
            root: root.into(),
            agent_id: agent_id.into(),
            allow_overlap: false,
            include_dotfiles: false,
        }
    }
}

/// One refusal, in the one shape.
pub(crate) fn birth_refusal(code: &str, reason: &str) -> serde_json::Value {
    json!({
        "ok": false,
        "schema": BIRTH_SCHEMA,
        "action": BIRTH_ACTION,
        "refused": code,
        "reason": reason,
    })
}

/// What a birth attempt WITHOUT an owner stamp gets, at any seam that has no
/// stamp to give — which is every generic transport seam there is.
///
/// The generic policy gate refuses first and refuses harder (the action sits at
/// `PositiveSovereign`), so this is defense in depth: a caller who reaches
/// `dispatch_tool` directly still cannot birth, because the dispatcher has no
/// `HumanOrigin` to hand the handler and cannot manufacture one.
pub fn birth_refusal_without_stamp() -> serde_json::Value {
    let mut refusal = birth_refusal(
        "human_gesture_required",
        "birth is the human's one-time gesture; the owner stamps its origin from a fact it \
         observes about itself, and a client-supplied origin string grants nothing",
    );
    if let Some(object) = refusal.as_object_mut() {
        object.insert("allowed_origins".into(), json!(BIRTH_HUMAN_ORIGINS));
        object.insert(
            "lesson".into(),
            json!(
                "birthing a brain is the human's gesture — the ceremony sends it; agents never do"
            ),
        );
    }
    refusal
}

/// Roots currently being born, by canonical key — single-flight per root (the
/// cp32 TOCTOU requirement), mirroring SPEC-1's own mechanism rather than
/// inventing a second one.
fn birth_in_flight_roots() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    static ROOTS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    ROOTS.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

/// Holds one root's single-flight claim and releases it on every exit path,
/// including a panic inside the first ingest.
pub struct BirthInFlightGuard(String);

impl Drop for BirthInFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut roots) = birth_in_flight_roots().lock() {
            roots.remove(&self.0);
        }
    }
}

/// `None` when another birth already holds this root.
fn claim_birth_root(canonical_root: &str) -> Option<BirthInFlightGuard> {
    let mut roots = birth_in_flight_roots()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !roots.insert(canonical_root.to_string()) {
        return None;
    }
    Some(BirthInFlightGuard(canonical_root.to_string()))
}

/// Hold a root's single-flight claim from a test, so exclusivity is proved
/// deterministically instead of by racing two threads and hoping.
#[cfg(test)]
pub(crate) fn claim_birth_root_for_test(canonical_root: &str) -> Option<BirthInFlightGuard> {
    claim_birth_root(canonical_root)
}

/// Perform the birth. Reachable only with an owner stamp, by construction.
///
/// BATTERY COMMIT: not implemented. The refusal below is today's literal truth —
/// the ceremony does not exist and `m1nd init` is still only `installSkills`
/// (`docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §2, last sentence).
pub fn run_birth(
    _registry: &crate::project_brains::ProjectBrainRegistry,
    _bound: &crate::session::SessionState,
    _request: &BirthRequest,
    _origin: HumanOrigin,
) -> m1nd_core::error::M1ndResult<serde_json::Value> {
    Ok(birth_refusal(
        "birth_not_implemented",
        "the P2 birth ceremony is not built in this binary",
    ))
}
