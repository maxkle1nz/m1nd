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
        object.insert("door".into(), json!(BIRTH_CEREMONY_COMMAND));
    }
    refusal
}

/// The command every refusal on the first-graph path must name.
///
/// It is a constant rather than prose repeated at each seam because the field
/// failure was not that any one message was wrong — each was correct — but that
/// FOUR different refusals all stopped short of the door, and an agent that
/// cannot see a way forward reports that the product does not work.
pub const BIRTH_CEREMONY_COMMAND: &str = "m1nd init --birth <repo>";

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

/// How long the commit waits for the staged brain to quiesce before it gives up
/// and refuses the birth.
///
/// The number is this caller's tolerance for a slow machine, never the
/// guarantee: a healthy quiesce returns the instant the last checkpoint ACK
/// lands and pays nothing for the headroom, while a genuinely stuck checkpoint
/// still fails closed. It is generous on purpose — the registry's own lifecycle
/// fixtures measured a five-second grace turning into rotating red on a loaded
/// two-core runner doing real embedding-cache and checkpoint I/O
/// (`TEST_PERSISTING_ACTOR_SHUTDOWN_GRACE`, `project_brains.rs`), and a birth is
/// a one-time human gesture that should wait rather than fail on a busy box.
const BIRTH_STAGING_QUIESCE_GRACE: std::time::Duration = std::time::Duration::from_secs(60);

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
    mut server: crate::server::McpServer,
    root: &str,
    agent_id: &str,
) -> m1nd_core::error::M1ndResult<serde_json::Value> {
    let (runtime_root, _project_root) = server.offline_operator_context();
    let registry_dir = server.config_registry_dir();
    let request = BirthRequest::ceremony(root, agent_id);

    // THE HOME CASE, decided before any store is chosen. See
    // [`home_birth_verdict`]: when this owner's runtime lives INSIDE the root the
    // human named, the brain for that root is this owner's OWN graph, and a
    // sidecar minted beside it is a brain nobody reads.
    match home_birth_verdict(&server, &runtime_root, &request)? {
        HomeBirth::NotHome => {}
        HomeBirth::Refuse(refusal) => return Ok(refusal),
        HomeBirth::Fill { canonical_root } => {
            return run_home_birth(server, &canonical_root, &request, HumanOrigin::Cli);
        }
    }

    let bound = server.into_session_state();
    let registry = crate::project_brains::ProjectBrainRegistry::new(
        runtime_root.join(crate::project_brains::PROJECT_BRAINS_DIR),
        registry_dir,
    );
    run_birth(&registry, &bound, &request, HumanOrigin::Cli)
}

/// Which door this ceremony takes.
enum HomeBirth {
    /// The runtime this owner boots from is not inside the named root: the
    /// hosted (project-brain) path applies, unchanged.
    NotHome,
    /// It is home, and there is nothing to do — the answer is the refusal.
    Refuse(serde_json::Value),
    /// It is home and the home is empty: fill this owner's own graph.
    Fill { canonical_root: String },
}

/// Is the root the human named the repo THIS OWNER'S RUNTIME LIVES IN?
///
/// THE DEFECT THIS ANSWERS, measured on 1.6.2. A user in a new repo, following
/// the README's own quickstart (`M1ND_RUNTIME_DIR=<repo>/.m1nd`, a plain
/// `m1nd-mcp --stdio` as their MCP server), ran `m1nd init --birth .`. It exited
/// 0 reporting a real node count — and their next session still served 0 nodes.
/// The ceremony had minted a PROJECT BRAIN under
/// `<runtime>/project-brains/<key>/`, and project brains are reached only by the
/// served owner's caller-root routing over HTTP (`http_server::resolve_brain`).
/// A plain stdio owner has no such routing: it serves the runtime's own graph
/// and nothing else. So the ceremony's whole output was invisible to the only
/// agent it was run for.
///
/// The discriminator is the runtime's own location, and it is the honest one:
/// if the runtime directory sits inside the named root, then this process IS
/// that repo's brain — there is no second brain to mint, only an empty one to
/// fill. It is a fact about THIS OWNER, never about the caller, so nothing here
/// weakens the cross-root sovereignty the owner ratified: a foreign root still
/// takes the hosted path with every one of its guards, generic `ingest` stays
/// refused for every client, and no agent gains a door.
fn home_birth_verdict(
    server: &crate::server::McpServer,
    runtime_root: &std::path::Path,
    request: &BirthRequest,
) -> m1nd_core::error::M1ndResult<HomeBirth> {
    use crate::project_brains::ProjectBrainRegistry;

    // `allow_overlap` and an unresolvable root are refused identically on both
    // doors, and `run_birth` states each in full. Deferring to it keeps ONE
    // wording per refusal instead of two that can drift apart.
    if request.allow_overlap {
        return Ok(HomeBirth::NotHome);
    }
    let trimmed = request.root.trim().trim_end_matches('/');
    if trimmed.is_empty() || !std::path::Path::new(trimmed).is_dir() {
        return Ok(HomeBirth::NotHome);
    }
    let key = ProjectBrainRegistry::canonical_key(trimmed);

    let runtime_key = ProjectBrainRegistry::canonical_key(&runtime_root.to_string_lossy());
    if !std::path::Path::new(&runtime_key).starts_with(std::path::Path::new(&key)) {
        return Ok(HomeBirth::NotHome);
    }

    // Home, but already inhabited. A brain is born once: a second ceremony must
    // not re-scan behind the human's back (that would be a silent `replace` on a
    // graph they may have grown for months), and must not mint a twin beside it.
    //
    // The test is the NODE COUNT, not `covers_root`. On this door those two ask
    // different questions: an EMPTY graph whose `workspace_root` happens to name
    // the repo — which is what a runtime placed at the repo root itself produces,
    // since the binding falls back to the graph path's parent — is covered and
    // still has no brain. Refusing it as "already has its brain" while reporting
    // zero nodes would be the same dishonesty this whole change is about.
    let bound = server.bound_boot_state()?;
    let node_count = u64::from(bound.graph.read().num_nodes());
    if node_count > 0 {
        let mut refusal = birth_refusal(
            "birth_root_is_bound_graph",
            "this repo already has its brain — the graph this owner serves IS that brain, so there \
             is nothing to birth; a birth would replace a graph that may have been growing for \
             months, and that is never this command's job",
        );
        if let Some(object) = refusal.as_object_mut() {
            object.insert("root".into(), json!(key));
            object.insert("node_count".into(), json!(node_count));
            object.insert("store_dir".into(), json!(runtime_key));
        }
        return Ok(HomeBirth::Refuse(refusal));
    }

    Ok(HomeBirth::Fill {
        canonical_root: key,
    })
}

/// Fill this owner's OWN empty graph with the repo it lives in — the human's
/// first ingest, through the human's own door.
///
/// It reuses the ordinary `ingest` handler rather than building a second
/// scanner, and commits it through the brain actor (see
/// `McpServer::ceremony_first_ingest` for why an offline file write is reverted
/// on the next boot). The shutdown afterwards is the cooperative
/// persist-before-release the stdio owner already runs, so the ceremony leaves
/// the runtime exactly as a clean session would.
fn run_home_birth(
    mut server: crate::server::McpServer,
    canonical_root: &str,
    request: &BirthRequest,
    origin: HumanOrigin,
) -> m1nd_core::error::M1ndResult<serde_json::Value> {
    // Single-flight per canonical root (the cp32 TOCTOU requirement), shared with
    // the hosted door so two ceremonies on one root can never interleave.
    let Some(_in_flight) = claim_birth_root(canonical_root) else {
        return Ok(birth_refusal(
            "birth_in_flight",
            "another birth of this root is already running; a second one would measure an empty \
             destination the first is about to fill",
        ));
    };

    let ingested = server.ceremony_first_ingest(json!({
        "agent_id": request.agent_id,
        "path": canonical_root,
        "include_dotfiles": request.include_dotfiles,
    }))?;
    server.shutdown()?;

    let node_count = ingested
        .get("node_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let edge_count = ingested
        .get("edge_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if let Some(empty) = refuse_an_empty_birth(canonical_root, node_count) {
        return Ok(empty);
    }

    Ok(json!({
        "ok": true,
        "schema": BIRTH_SCHEMA,
        "action": BIRTH_ACTION,
        "born_root": canonical_root,
        // Absolute, because a receipt that names `.m1nd` tells a human nothing
        // about which `.m1nd` on their disk now holds their brain.
        "store_dir": crate::project_brains::ProjectBrainRegistry::canonical_key(
            &server.offline_operator_context().0.to_string_lossy(),
        ),
        "origin": origin.as_str(),
        "node_count": node_count,
        "edge_count": edge_count,
        // The brain that was filled. Named, because the two doors write to
        // different places and a human reading a receipt should never have to
        // guess which one answered.
        "brain": "owner_bound_graph",
        "routes_by_caller_root": true,
        "dev_graph_untouched": false,
        "honest_limits": [
            "this runtime lives inside the repo you named, so the brain for that repo IS this owner's own graph — there was no second brain to mint, only an empty one to fill",
            "birth is the human's gesture: the owner stamps its origin from the ceremony ingress, and no client payload can produce that stamp",
            "it stops an agent from creating a brain by habit or with a disguised request — it is not a defense against a hostile local process running as you",
            "birth happens once; keep the graph fresh afterwards with ingest {mode:\"refresh\"} from this exact root",
        ],
    }))
}

/// THE HONESTY GATE both doors pass through: a ceremony that produced no graph
/// never reports success.
///
/// On 1.6.2 `m1nd init --birth .` could exit 0 over an empty graph, which is
/// worse than refusing — the human walks away believing they have a brain. An
/// ingest that yields zero nodes is a real, common condition (an empty repo, a
/// root one level too high or too low, a tree the extractors do not read), and
/// every one of those is a thing the human can check and fix.
fn refuse_an_empty_birth(canonical_root: &str, node_count: u64) -> Option<serde_json::Value> {
    if node_count > 0 {
        return None;
    }
    let mut refusal = birth_refusal(
        "birth_produced_empty_graph",
        "the scan of that root produced no nodes, so there is no brain to report; a ceremony that \
         exits successfully over an empty graph leaves you believing you have one",
    );
    if let Some(object) = refusal.as_object_mut() {
        object.insert("root".into(), json!(canonical_root));
        object.insert(
            "check".into(),
            json!([
                "is that the repository root, and does it hold source files?",
                "are the files you expect ignored (dotfiles are skipped unless include_dotfiles is set)?",
                "does m1nd have an extractor for this repo's languages (see the language table in README.md)?",
            ]),
        );
    }
    Some(refusal)
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
/// cannot see. The staged brain is then brought to a stop, so the bytes about to
/// move belong to nobody. The destination then appears in exactly ONE step — a
/// directory `rename` within the registry's own base dir, hence the same
/// filesystem, hence atomic. So a `kill -9` at any instant leaves the
/// destination ABSENT or WHOLE: there is no window in which a half-built store
/// sits where a later boot would warm-boot it. That is §5.8's birth half, and it
/// is a property of the shape rather than of a recovery routine that has to run
/// and could fail to.
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
    let staged_store = staging_registry.store_dir_for(&key);
    let (brain, ingest_result, reused) = staging_registry.bootstrap(&key, &ingest_args)?;
    debug_assert!(!reused, "a staging registry can never reuse a brain");

    // 8. QUIESCE. A newly bootstrapped brain is LIVE: its actor holds a
    //    checkpoint store rooted inside the staged directory, and that store
    //    keeps a directory handle plus its writer lock and leases open for as
    //    long as the brain exists. Renaming a tree with open handles is legal on
    //    Unix and refused on Windows (`ERROR_SHARING_VIOLATION`, os error 32),
    //    so a birth that skips this step is a birth that only ever worked on
    //    two of the three CI legs. The registry's own graceful stop is what
    //    releases them — pause, checkpoint with an ACK, stop the actor, release
    //    the hosted instance entry, drop the cell — so the staged store is inert
    //    bytes before anything moves it. Our own reference goes first, so the
    //    stop below is the last holder; a failure to quiesce refuses the birth
    //    rather than committing a store something is still writing to.
    drop(brain);
    staging_registry.shutdown(BIRTH_STAGING_QUIESCE_GRACE)?;
    drop(staging_registry);

    // 9. COMMIT: one rename, within one base dir, therefore one filesystem,
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
    if let Some(empty) = refuse_an_empty_birth(&key, node_count) {
        return Ok(empty);
    }

    Ok(json!({
        "ok": true,
        "schema": BIRTH_SCHEMA,
        "action": BIRTH_ACTION,
        "born_root": key,
        "store_dir": store.to_string_lossy(),
        "origin": origin.as_str(),
        "node_count": node_count,
        "edge_count": edge_count,
        "brain": "project_brain",
        // The facts a human needs in order to believe the ceremony worked, each
        // one measured here rather than hoped for.
        "routes_by_caller_root": registry.knows(&key),
        "dev_graph_untouched": true,
        // How to REACH what was just born. A hosted brain is served by this
        // owner's caller-root routing, so an agent gets it by attaching to this
        // owner from inside that repo — never by starting its own stdio owner on
        // a different runtime, which would see an empty graph and report that the
        // ceremony did nothing.
        "reach_it_with": "m1nd-mcp --attach auto --stdio   # run from inside that repo",
        "honest_limits": [
            "birth is the human's gesture: the owner stamps its origin from the ceremony ingress, and no client payload can produce that stamp",
            "it stops an agent from creating a brain by habit or with a disguised request — it is not a defense against a hostile local process running as you",
            "adopting an existing brain is migration, a boot-time fact with no verb; birth only ever writes into an empty destination",
            "this brain is hosted by THIS owner: agents reach it by attaching to it from inside that repo, not by booting their own runtime there",
        ],
    }))
}

/// A plain, human-facing one line for a successful birth, printed to stderr next
/// to the machine JSON on stdout. A newcomer who runs `m1nd init --birth` should
/// read a sentence, not parse a receipt. Success only — callers print it when the
/// payload's `ok` is true.
pub fn birth_receipt_human_line(payload: &serde_json::Value) -> String {
    let root = payload
        .get("born_root")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("this repo");
    let nodes = payload
        .get("node_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let edges = payload
        .get("edge_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    format!(
        "m1nd: done — {root} now has a graph of {nodes} nodes and {edges} edges. \
         Reach it with an agent from inside that repo: m1nd-mcp --attach auto --stdio"
    )
}

#[cfg(test)]
mod human_line_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn birth_human_line_speaks_plainly_and_names_the_next_step() {
        let line = birth_receipt_human_line(&json!({
            "ok": true,
            "born_root": "/tmp/acme",
            "node_count": 6453u64,
            "edge_count": 21012u64,
        }));
        assert!(line.contains("/tmp/acme"), "names the repo: {line}");
        assert!(line.contains("6453"), "carries the real node count: {line}");
        assert!(
            line.contains("m1nd-mcp --attach auto"),
            "tells them how to reach it: {line}"
        );
        // No apparatus vocabulary bleeds into the sentence a newcomer reads.
        for leak in ["honest_limits", "schema", "reflex", "POSITIVE_SOVEREIGN"] {
            assert!(
                !line.contains(leak),
                "human line must not leak `{leak}`: {line}"
            );
        }
    }

    #[test]
    fn birth_human_line_survives_a_missing_root() {
        let line = birth_receipt_human_line(&json!({ "ok": true, "node_count": 1u64 }));
        assert!(
            line.contains("this repo"),
            "falls back to a plain noun: {line}"
        );
    }
}
