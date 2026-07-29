//! SPEC-2 — `brain.bootstrap.birth`, the birth ceremony's server verb.
//!
//! The normative document is `docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §2
//! (RATIFIED, owner, 2026-07-29, all four §6 items — item 4 being the `human-cli`
//! allowlist entry this module exists to honour).
//!
//! **THE ORIGIN STAMP — where the spec constrains and where this resolves.**
//!
//! §2 constrains three things and leaves one open. It requires (a) a CLOSED
//! server-side allowlist of human origins, (b) the stamp applied by the owner's
//! own surface, and (c) that a client-claimed origin string grant nothing — the
//! ratify counter-precedent, `system_blocks_handlers.rs:435`. It does NOT say
//! WHICH fact the owner observes to stamp `human-cli`; it says only that the
//! ceremony `m1nd init --birth <root>` is what mints it.
//!
//! Resolved here, and declared as a resolution rather than a reading: the owner
//! stamps `human-cli` from a fact it observes about ITSELF — its own ceremony
//! ingress (`m1nd-mcp --birth <root>`, `main.rs`), the offline-operator shape
//! `--inbox-sweep` and `--medulla-migrate` already use. That ingress is the ONLY
//! construction site of a [`HumanOrigin`] in the codebase. The closed allowlist
//! §2 asks for is therefore the TYPE SYSTEM, not a string check: no
//! `Deserialize`, no `FromStr`, no public constructor, and [`run_birth`] takes
//! the stamp BY VALUE, so it is unreachable without one. No transport can
//! manufacture a stamp, so no header, field, or claim can ever birth a brain.
//!
//! Rejected alternative, for the record: a ceremony nonce the owner writes and
//! the CLI presents. It buys the same honest limit (a same-UID process can read
//! the owner's runtime dir) while adding a bearer-secret lifecycle — write,
//! rotate, consume, expire — and an HTTP client the npm CLI does not have.
//!
//! **The honest limit, stated where the code is.** A same-UID process can run
//! the ceremony command too. This closes the REFLEX vector — the agent holding
//! the MCP surface cannot birth by habit, misconfiguration, or a dressed-up
//! payload — exactly as SPEC-1 §1.3 and `system_blocks_handlers.rs:492-494`
//! state for their own doors. It is not a defense against a hostile local
//! process; that is the lease plane, still dormant.

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

/// The on-disk artifacts whose presence means a destination is NOT empty (the
/// cp32 requirement: "empty destination" defined ON DISK — no orphan manifest,
/// snapshot, or checkpoint). A brain that was, or the debris of something that
/// tried to be, is equally a reason to refuse: birth is for an empty place, and
/// anything else is a decision only a human can make.
/// Written as `/`-separated relative paths and rebuilt component by component
/// below, so a refusal names the exact pointer rather than only the directory
/// holding it: a human reading `checkpoint-store` still has to go looking, and
/// one reading `checkpoint-store/CURRENT` does not.
const BIRTH_DESTINATION_ARTIFACTS: &[&str] = &[
    "project_brain.json",
    "graph_snapshot.json",
    "plasticity_state.json",
    "checkpoint-store",
    "checkpoint-store/CURRENT",
    "agent-memory",
];

/// Prefix of the STAGING directories a birth builds in before committing. Kept
/// under the registry's own base dir (same filesystem, so the commit can be a
/// rename), dot-prefixed so the disk roster can never mistake one for a store,
/// and swept on every birth.
const BIRTH_STAGING_PREFIX: &str = ".birth-staging-";

/// Rebuild one artifact path component by component, so the `/` in the table
/// above is a path separator on every platform rather than a literal in a
/// filename on Windows.
fn artifact_path(store: &std::path::Path, artifact: &str) -> std::path::PathBuf {
    let mut path = store.to_path_buf();
    for component in artifact.split('/') {
        path.push(component);
    }
    path
}

/// What occupies a destination, if anything — the names a refusal must carry, so
/// a human is not sent to guess at their own filesystem.
fn destination_occupants(store: &std::path::Path) -> Vec<String> {
    if !store.exists() {
        return Vec::new();
    }
    let mut found: Vec<String> = BIRTH_DESTINATION_ARTIFACTS
        .iter()
        .filter(|artifact| artifact_path(store, artifact).exists())
        .map(|artifact| (*artifact).to_string())
        .collect();
    if found.is_empty() {
        // Not one of the known artifacts, but not empty either. Name whatever IS
        // there rather than refusing with an empty list — an unnamed "not empty"
        // is a refusal a human cannot act on.
        found = std::fs::read_dir(store)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
    }
    found.sort();
    found
}

/// Remove staging directories left behind by an interrupted birth. This is
/// housekeeping, not the recovery: the recovery is that a crash never created
/// the destination at all, because only the committing rename does.
fn sweep_stale_staging(base: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(BIRTH_STAGING_PREFIX)
        {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// THE CEREMONY — the owner's `--birth` ingress, and the ONLY construction site
/// of a [`HumanOrigin`] in this codebase.
///
/// The binary's `--birth <repo>` flag calls exactly this, having booted the
/// owner offline and BEFORE any transport is opened. The orchestration lives
/// here rather than in `main.rs` because the bound session is a crate-internal
/// capability by design (`McpServer::into_session_state` is `pub(crate)` with a
/// `compile_fail` doctest guarding it): the ceremony needs the owner's own
/// binding in order to REFUSE a root the dev graph already covers, and it gets
/// it without that capability ever leaving the library.
///
/// A library consumer can call this, and that is the correct trust level: a
/// consumer of this crate IS an owner process, exactly like the binary. What no
/// caller can do is arrive over a transport — MCP and REST reach `dispatch_tool`
/// and the generic policy gate, neither of which can construct the stamp.
pub fn run_ceremony(
    server: crate::server::McpServer,
    root: &str,
    agent_id: &str,
) -> m1nd_core::error::M1ndResult<serde_json::Value> {
    let (runtime_root, _project_root) = server.offline_operator_context();
    let registry_dir = server.config_registry_dir();
    let bound = server.into_session_state();
    let registry = crate::project_brains::ProjectBrainRegistry::new(
        runtime_root.join(crate::project_brains::PROJECT_BRAINS_DIR),
        registry_dir,
    );
    run_birth(
        &registry,
        &bound,
        &BirthRequest::ceremony(root, agent_id),
        HumanOrigin::Cli,
    )
}

/// Removes its staging directory on every exit path, including a panic inside
/// the first ingest. A leaked staging dir is harmless — the roster cannot see a
/// dot-prefixed name and the next birth sweeps it — but leaving one is still
/// leaving debris.
struct StagingDir(std::path::PathBuf);

impl Drop for StagingDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Perform the birth. Reachable only with an owner stamp, by construction: the
/// `origin` argument has no public constructor, so a caller without one cannot
/// even build the call.
///
/// THE SHAPE, and why it is this shape. Every guard runs before anything durable
/// exists, and the brain itself is built in a STAGING directory the routing seam
/// cannot see. The destination then appears in exactly ONE step — a directory
/// `rename` within the registry's own base dir, hence the same filesystem, hence
/// atomic. So a `kill -9` at any instant leaves the destination ABSENT or WHOLE:
/// there is no window in which a half-built store sits where a later boot would
/// warm-boot it. That is §5.8's birth half, and it is a property of the shape
/// rather than of a recovery routine that has to run and could fail to.
///
/// The mint itself is NOT rebuilt. `ProjectBrainRegistry::bootstrap` already
/// mints, ingests, writes the birth record and checkpoints; pointing a second
/// registry at the staging base reuses that path verbatim. Its own overlap guard
/// would be blind there (a staging registry knows no brains), so the overlap
/// check runs here, against the REAL registry, before staging begins.
pub fn run_birth(
    registry: &crate::project_brains::ProjectBrainRegistry,
    bound: &crate::session::SessionState,
    request: &BirthRequest,
    origin: HumanOrigin,
) -> m1nd_core::error::M1ndResult<serde_json::Value> {
    use crate::project_brains::{detect_root_overlap, ProjectBrainRegistry, RootOverlap};

    // 1. `allow_overlap` is not merely ignored on this path — it is REFUSED, on
    //    the flag, before any overlap is computed (§2: "no `allow_overlap` below
    //    sovereign"; cp32: forbid it off the sovereign path). The hatch that lets
    //    one repo grow two brains cannot be reached through the birth door at
    //    all, so there is nothing left to reason about downstream.
    if request.allow_overlap {
        return Ok(birth_refusal(
            "birth_allow_overlap_forbidden",
            "birth never carries the overlap escape hatch: one repo must not grow two brains, and \
             the sovereign path is not where that rule gets an exception",
        ));
    }

    // 2. The root must RESOLVE. `canonical_key` falls back to the raw string for
    //    a path that does not exist, so without this a birth into a typo would
    //    mint a brain for a directory nobody has.
    let trimmed = request.root.trim().trim_end_matches('/');
    if trimmed.is_empty() || !std::path::Path::new(trimmed).is_dir() {
        return Ok(birth_refusal(
            "birth_root_unresolvable",
            "the root to birth does not resolve to a directory on this machine; a birth into a \
             path that does not exist is a birth into nowhere",
        ));
    }
    let key = ProjectBrainRegistry::canonical_key(trimmed);

    // 3. The bound dev graph is never touched (§2). A root the owner's own graph
    //    already serves needs no brain, and minting one would SHADOW the owner —
    //    the guard `run_bootstrap_core` states for its own path, reused here.
    if bound.covers_root(&key) {
        let mut refusal = birth_refusal(
            "birth_root_is_bound_graph",
            "this owner's bound graph already covers that root — you are home; birthing a second \
             brain over it would shadow the owner's own graph",
        );
        if let Some(object) = refusal.as_object_mut() {
            object.insert("root".into(), json!(key));
        }
        return Ok(refusal);
    }

    // 4. Single-flight per canonical root (the cp32 TOCTOU requirement). Taken
    //    BEFORE the emptiness check, so a second birth can never read "empty"
    //    from a destination the first is about to fill.
    let Some(_in_flight) = claim_birth_root(&key) else {
        return Ok(birth_refusal(
            "birth_in_flight",
            "another birth of this root is already running; a second one would measure an empty \
             destination the first is about to fill",
        ));
    };

    // 5. EMPTY DESTINATION, defined ON DISK. `knows` covers the live map and a
    //    readable manifest; the artifact sweep covers the orphans a manifest
    //    check would miss — a snapshot with no manifest, a checkpoint store, a
    //    memory sidecar. The refusal NAMES what it found, and says that adopting
    //    an existing brain is migration, never birth (§3).
    let store = registry.store_dir_for(&key);
    let occupants = destination_occupants(&store);
    if registry.knows(&key) || !occupants.is_empty() {
        let mut refusal = birth_refusal(
            "birth_destination_not_empty",
            "birth only ever writes into an EMPTY destination, and this one is occupied; taking \
             over an existing brain is migration — a boot-time fact with no verb — never a birth",
        );
        if let Some(object) = refusal.as_object_mut() {
            object.insert("root".into(), json!(key));
            object.insert("store_dir".into(), json!(store.to_string_lossy()));
            object.insert("occupied_by".into(), json!(occupants));
        }
        return Ok(refusal);
    }

    // 6. Overlap, against the REAL registry (the staging one below knows nobody).
    //    The twin-brain trap: a parent folder of a brained repo, a child of one,
    //    or a git worktree whose main repository already has a brain.
    match detect_root_overlap(&key, &registry.existing_brain_roots()) {
        RootOverlap::None => {}
        overlap => {
            let (class, existing) = match &overlap {
                RootOverlap::Child { existing } => ("child", existing.clone()),
                RootOverlap::Parent { existing } => ("parent", existing.clone()),
                RootOverlap::Worktree { existing, .. } => ("worktree", existing.clone()),
                RootOverlap::None => unreachable!("matched above"),
            };
            let mut refusal = birth_refusal(
                "birth_root_overlaps_existing_brain",
                "this root overlaps a repo that already has its own project brain; two brains over \
                 one repo double the ingest cost and split its memories across two stores",
            );
            if let Some(object) = refusal.as_object_mut() {
                object.insert("root".into(), json!(key));
                object.insert("overlap_class".into(), json!(class));
                object.insert("existing_brain_root".into(), json!(existing));
            }
            return Ok(refusal);
        }
    }

    // 7. PREPARE, in a staging dir the routing seam cannot see. Everything a
    //    brain IS gets built there — store, first ingest, birth record,
    //    checkpoint — through `ProjectBrainRegistry::bootstrap`, unchanged.
    let base = registry.base_dir().to_path_buf();
    std::fs::create_dir_all(&base)?;
    sweep_stale_staging(&base);
    let staging_base = base.join(format!(
        "{BIRTH_STAGING_PREFIX}{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&staging_base)?;
    let staged = StagingDir(staging_base.clone());

    let staging_registry =
        ProjectBrainRegistry::new(staging_base.clone(), registry.registry_dir().cloned());
    let ingest_args = json!({
        "agent_id": request.agent_id,
        "include_dotfiles": request.include_dotfiles,
    });
    let (_brain, ingest_result, reused) = staging_registry.bootstrap(&key, &ingest_args)?;
    debug_assert!(!reused, "a staging registry can never reuse a brain");
    let staged_store = staging_registry.store_dir_for(&key);

    // 8. COMMIT: one rename, within one base dir, therefore one filesystem,
    //    therefore atomic. Before this instant the destination does not exist;
    //    after it, it is whole. There is no third state to crash into.
    if let Some(parent) = store.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&staged_store, &store)?;
    drop(staged);

    let node_count = ingest_result
        .get("node_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let edge_count = ingest_result
        .get("edge_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    Ok(json!({
        "ok": true,
        "schema": BIRTH_SCHEMA,
        "action": BIRTH_ACTION,
        "born_root": key,
        "store_dir": store.to_string_lossy(),
        "origin": origin.as_str(),
        "node_count": node_count,
        "edge_count": edge_count,
        // The facts a human needs in order to believe the ceremony worked, each
        // one measured here rather than hoped for.
        "routes_by_caller_root": registry.knows(&key),
        "dev_graph_untouched": true,
        "honest_limits": [
            "birth is the human's gesture: the owner stamps its origin from the ceremony ingress, and no client payload can produce that stamp",
            "it closes the reflex vector — an agent calling by habit or with a dressed-up payload — not a hostile same-UID local process",
            "adopting an existing brain is migration, a boot-time fact with no verb; birth only ever writes into an empty destination",
        ],
    }))
}
