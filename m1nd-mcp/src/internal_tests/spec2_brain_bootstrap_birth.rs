//! SPEC-2 — `brain.bootstrap.birth`, the BIRTH path.
//!
//! The normative document is `docs/GENESIS-INGEST-CONSUMERS-SPEC.md` (RATIFIED,
//! owner, 2026-07-29, all four §6 items — item 4 is this file's subject). This
//! file IS its §5.7 and the birth half of §5.8, which SPEC-1's battery left
//! deliberately absent. Written before the implementation, born RED against
//! today's binary, and never edited afterwards to make the implementation pass.
//!
//! What the verb is, in one line: the ONE way a brain is born, and it is not an
//! agent's to call. Admission is an OWNER-STAMPED human origin (§2) — the owner
//! stamps from a fact it observes about itself, never from a field a client
//! sends. Every generic transport seam therefore refuses it, always.
//!
//! WHY THE POLICY GATE IS IN THE REFUSAL CALLS. `enforce_generic_action_policy`
//! is the pure, pre-brain admission seam (spec R-I) and is what both transports
//! run before `dispatch_tool`. §5.7's first two cases are about exactly that
//! seam: a birth attempt is refused there, and a birth attempt DECORATED with a
//! client-claimed origin is refused with the same bytes — because the claim
//! never enters the classification at all.
//!
//! Numbering follows the spec's own §5 list so a reader can check the battery
//! against the document item by item.

use crate as m1nd_mcp;

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::brain_birth::{
    self, BirthRequest, HumanOrigin, BIRTH_HUMAN_ORIGINS, BIRTH_ORIGINS_WITH_A_STAMPING_SEAM,
};
use m1nd_mcp::project_brains::ProjectBrainRegistry;
use m1nd_mcp::server::{dispatch_tool, enforce_generic_action_policy, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::json;
use std::path::{Path, PathBuf};

const AGENT: &str = "spec2-birth-probe";

/// The verb's advertised MCP name.
const BIRTH_TOOL: &str = "brain_birth";

/// The refusal EVERY generic seam must produce for a birth attempt. Written out
/// once, asserted from several angles: the bytes are the contract.
///
/// The closing sentence is part of that contract, not decoration. A wire client
/// asking to birth a brain is asking for the one thing only a human can do, and
/// until 1.6.2 the refusal ended without saying so — measured in the field, an
/// agent collected four such refusals and concluded the product was unusable.
/// Naming the ceremony grants nothing: the stamp lives in the CLI ingress, and
/// no sentence in a refusal can manufacture one.
const GENERIC_BIRTH_REFUSAL: &str = "invalid params for brain_birth: \
     generic_action_authority_required: semantic_action=brain.bootstrap.birth \
     authority_floor=POSITIVE_SOVEREIGN cannot use generic REST/MCP dispatch; \
     no exact typed G2/G3 lease consumer is installed for this action. The first \
     graph is born on the human's explicit pick: ask them (create new × load \
     existing — always), then with their yes run `m1nd init --birth <repo>` \
     yourself";

/// The owner's own bound brain (the "dev graph" §2 says birth never touches),
/// with a runtime under `runtime` and no roots declared.
fn build_bound_state(runtime: &Path) -> SessionState {
    std::fs::create_dir_all(runtime).expect("runtime dir");
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        registry_dir: Some(runtime.join("registry")),
        ..McpConfig::default()
    };
    SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("init session")
}

/// The project-brain registry the owner would host births in — rooted under the
/// same runtime dir the served owner uses (`PROJECT_BRAINS_DIR`).
fn build_registry(runtime: &Path) -> ProjectBrainRegistry {
    ProjectBrainRegistry::with_capacity(
        runtime.join(m1nd_mcp::project_brains::PROJECT_BRAINS_DIR),
        Some(runtime.join("registry")),
        4,
    )
}

/// A small, deterministic Rust crate — the repo a birth ingests.
fn write_repo(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"spec2fixture\"\nversion = \"0.0.0\"\n",
    )
    .expect("Cargo.toml");
    std::fs::write(
        root.join("src/lib.rs"),
        "pub mod helper;\npub fn top() -> i64 { helper::help() + 1 }\n\
         pub struct Top { pub v: i64 }\npub fn second() -> i64 { 2 }\n",
    )
    .expect("lib.rs");
    std::fs::write(
        root.join("src/helper.rs"),
        "pub fn help() -> i64 { 41 }\npub struct Helper { pub v: i64 }\n\
         pub fn helper_two() -> i64 { 7 }\n",
    )
    .expect("helper.rs");
}

fn canonical(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

/// Seed the bound brain the way the field seeds one — the trusted library
/// ingest production `ingest` itself uses — and declare its root, so the "dev
/// graph is never touched" assertions have a real graph to be untouched.
fn seed_bound_graph(state: &mut SessionState, repo: &Path) {
    let (graph, _) = m1nd_ingest::Ingestor::new(m1nd_ingest::IngestConfig {
        root: repo.to_path_buf(),
        parallelism: 1,
        ..m1nd_ingest::IngestConfig::default()
    })
    .ingest()
    .expect("trusted fixture ingest");
    {
        let mut live = state.graph.write();
        *live = graph;
        if !live.finalized {
            live.finalize().expect("finalize seeded graph");
        }
    }
    state.rebuild_engines().expect("rebuild engines");
    let declared = canonical(repo);
    state.ingest_roots = vec![declared.clone()];
    state.workspace_root = Some(declared);
    state.persist().expect("persist the seeded bound brain");
}

/// The birth, driven the ONLY way it can be driven: with an owner stamp the
/// ceremony ingress constructs. No transport can reach this.
fn ceremony_birth(
    registry: &ProjectBrainRegistry,
    bound: &SessionState,
    root: &str,
) -> serde_json::Value {
    brain_birth::run_birth(
        registry,
        bound,
        &BirthRequest::ceremony(root, AGENT),
        HumanOrigin::Cli,
    )
    .expect("the ceremony must answer, refusal or receipt")
}

/// Params a CLIENT would send. Every field a client could dress a birth up with,
/// so §5.7's "refused identically" is asserted against a real attempt at forgery.
fn claimed_origin_params(root: &str) -> serde_json::Value {
    json!({
        "root": root,
        "agent_id": AGENT,
        "birth_via": "human-cli",
        "origin": "human-ui",
        "imported_via": "human-touchid",
        "ratified_via": "human-ui",
    })
}

fn plain_params(root: &str) -> serde_json::Value {
    json!({ "root": root, "agent_id": AGENT })
}

// ---------------------------------------------------------------------------
// §5.7 (a) — birth without an owner-stamped origin is refused
// ---------------------------------------------------------------------------

/// The generic MCP/REST seam has no stamp to give and cannot manufacture one, so
/// the birth verb is refused there — at the pure policy gate, before brain
/// resolution, at the ratified `PositiveSovereign` floor.
#[test]
fn spec2_5_7a_birth_without_owner_stamped_origin_is_refused() {
    let refusal = enforce_generic_action_policy(BIRTH_TOOL, &plain_params("/tmp/anywhere"))
        .expect_err("a birth over generic dispatch must be refused")
        .to_string();

    assert_eq!(refusal, GENERIC_BIRTH_REFUSAL);
}

// ---------------------------------------------------------------------------
// §5.7 (b) — a client-CLAIMED origin is refused IDENTICALLY
// ---------------------------------------------------------------------------

/// The ratify counter-precedent (`system_blocks_handlers.rs:435`), made
/// mechanical: "a client-supplied origin token (including 'human-ui') grants no
/// authority". Byte equality is the assertion, not mere similarity — a claimed
/// origin must buy NOTHING, not even a distinguishable refusal that would tell a
/// caller its guess was the right shape.
#[test]
fn spec2_5_7b_client_claimed_origin_is_refused_identically() {
    let plain = enforce_generic_action_policy(BIRTH_TOOL, &plain_params("/tmp/anywhere"))
        .expect_err("plain birth refused")
        .to_string();
    let dressed =
        enforce_generic_action_policy(BIRTH_TOOL, &claimed_origin_params("/tmp/anywhere"))
            .expect_err("a birth dressed in every human-origin token must still be refused")
            .to_string();

    assert_eq!(dressed, plain, "a claimed origin must buy nothing at all");
    assert_eq!(dressed, GENERIC_BIRTH_REFUSAL);
}

/// Every token on the ratified allowlist, tried as a claim. A closed list is
/// only closed if being ON it grants nothing when the client is the one saying
/// it — which is the whole difference between SPEC-2's gate and
/// `receipt_import`'s.
#[test]
fn spec2_5_7b2_every_allowlisted_token_grants_nothing_when_the_client_claims_it() {
    let plain = enforce_generic_action_policy(BIRTH_TOOL, &plain_params("/tmp/anywhere"))
        .expect_err("plain birth refused")
        .to_string();
    for token in BIRTH_HUMAN_ORIGINS {
        let claimed = enforce_generic_action_policy(
            BIRTH_TOOL,
            &json!({ "root": "/tmp/anywhere", "agent_id": AGENT, "birth_via": token }),
        )
        .expect_err("a claimed allow-listed origin must still be refused")
        .to_string();
        assert_eq!(claimed, plain, "claiming {token} changed the answer");
    }
}

/// Defense in depth: a caller that reaches `dispatch_tool` DIRECTLY — past the
/// gate, the way an in-process seam could — still cannot birth THERE, because
/// the dispatcher holds no owner context. But by the owner's law (2026-08-02:
/// an agent MAY birth with the human's authorization, and must always offer
/// create-new × load-existing) the answer is no longer "agents never do": it
/// is the agent's real path — ask the human, then run the ceremony yourself.
#[test]
fn spec2_5_7b3_direct_dispatch_answers_with_the_agents_path_not_a_dead_end() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_bound_state(&temp.path().join("runtime"));
    let repo = temp.path().join("repo");
    write_repo(&repo);

    let payload = dispatch_tool(
        &mut state,
        BIRTH_TOOL,
        &claimed_origin_params(&canonical(&repo)),
    )
    .expect("the dispatcher must answer with a refusal payload, not an unknown tool");

    assert_eq!(payload["ok"], json!(false));
    assert_eq!(payload["refused"], json!("human_authorization_required"));
    assert_eq!(payload["allowed_origins"], json!(BIRTH_HUMAN_ORIGINS));
    let path = payload
        .get("your_path")
        .expect("the answer must carry the agent's actionable path");
    let steps = serde_json::to_string(path).expect("encode path");
    assert!(
        steps.contains("create-new") && steps.contains("load-existing"),
        "the path must offer both choices by name: {steps}"
    );
    assert!(
        steps.contains("m1nd init --birth"),
        "the path must name the exact command the authorized agent runs: {steps}"
    );
}

/// The allowlist is the ratified one, and the binary is honest about which of
/// its entries it can actually stamp today. `receipt_import`'s const carries the
/// same discipline in prose; here a test holds it.
#[test]
fn spec2_5_7b4_the_allowlist_is_the_ratified_one_and_names_its_installed_stamp() {
    assert_eq!(
        BIRTH_HUMAN_ORIGINS,
        ["human-ui", "human-touchid", "human-cli", "human-via-agent"],
        "§2's closed allowlist, ratified §6 item 4, extended by the owner's \
         2026-08-02 law (an agent relaying the human's explicit yes)"
    );
    assert_eq!(
        BIRTH_ORIGINS_WITH_A_STAMPING_SEAM,
        ["human-cli"],
        "only the P2 ceremony has a stamping seam in this binary; adding another \
         is a code change plus a test, never a client string"
    );
    for installed in BIRTH_ORIGINS_WITH_A_STAMPING_SEAM {
        assert!(
            BIRTH_HUMAN_ORIGINS.contains(installed),
            "{installed} can be stamped but is not on the ratified allowlist"
        );
    }
}

// ---------------------------------------------------------------------------
// §5.7 (c) — the ceremony, on an empty destination
// ---------------------------------------------------------------------------

/// The happy path: a stamped birth into a destination that is empty ON DISK
/// creates the brain, ingests the repo, and hands back a certificate.
#[test]
fn spec2_5_7c_ceremony_on_an_empty_destination_births_the_brain() {
    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    let bound = build_bound_state(&runtime);
    let registry = build_registry(&runtime);
    let repo = temp.path().join("newborn");
    write_repo(&repo);
    let key = canonical(&repo);

    // Empty destination, on disk, before the ceremony.
    assert!(
        !registry.knows(&key),
        "precondition: no brain for this root"
    );
    assert!(
        !registry.store_dir_for(&key).exists(),
        "precondition: the destination store dir must not exist yet"
    );

    let receipt = ceremony_birth(&registry, &bound, &key);

    assert_eq!(receipt["ok"], json!(true), "receipt was {receipt}");
    assert_eq!(receipt["schema"], json!(brain_birth::BIRTH_SCHEMA));
    assert_eq!(receipt["action"], json!(brain_birth::BIRTH_ACTION));
    assert_eq!(receipt["born_root"], json!(key));
    assert_eq!(receipt["origin"], json!("human-cli"));
    assert!(
        receipt["node_count"].as_u64().unwrap_or(0) > 0,
        "a birth includes the first ingest: {receipt}"
    );
    // The brain exists ON DISK, not merely in a map.
    let store = registry.store_dir_for(&key);
    assert!(
        store.join("project_brain.json").is_file(),
        "the birth record must be on disk at {store:?}"
    );
    assert!(
        store.join("graph_snapshot.json").is_file(),
        "the first ingest must be durable at {store:?}"
    );
    assert_eq!(receipt["store_dir"], json!(store.to_string_lossy()));
}

// ---------------------------------------------------------------------------
// §5.7 (d) — the born brain routes by caller root
// ---------------------------------------------------------------------------

/// A birth that does not ROUTE is a directory, not a brain. After the ceremony,
/// a caller whose root is the born root resolves to the new brain — the same
/// `knows`/`try_resolve` path the owner's routing seam uses.
#[test]
fn spec2_5_7d_the_born_brain_routes_by_caller_root() {
    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    let bound = build_bound_state(&runtime);
    let registry = build_registry(&runtime);
    let repo = temp.path().join("newborn");
    write_repo(&repo);
    let key = canonical(&repo);

    let receipt = ceremony_birth(&registry, &bound, &key);
    assert_eq!(receipt["ok"], json!(true), "receipt was {receipt}");

    assert!(
        registry.knows(&key),
        "the owner must recognise the born root as a brain it hosts"
    );
    // A COLD registry over the same base dir — the served owner, restarted.
    // Routing must come from what the birth wrote to disk, never from a warm map
    // the ceremony happened to leave behind.
    let cold = build_registry(&runtime);
    assert!(
        cold.knows(&key),
        "a restarted owner must still route this root — the birth record is on disk"
    );
    let nodes = receipt["node_count"].as_u64().expect("receipt node_count");
    assert!(
        cold.disk_roster()
            .iter()
            .any(|(root, _facts, _dir)| root == &key),
        "the born brain must appear in the owner's disk roster"
    );
    assert!(nodes > 0);
}

// ---------------------------------------------------------------------------
// §5.7 (e) — the dev graph is never touched
// ---------------------------------------------------------------------------

/// §2: "the bound dev graph is never touched". Two halves, both asserted: a
/// birth elsewhere leaves the bound graph byte-identical, and a birth AT the
/// bound graph's own root refuses rather than shadowing it.
#[test]
fn spec2_5_7e_the_bound_dev_graph_is_never_touched() {
    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    let mut bound = build_bound_state(&runtime);
    let dev_repo = temp.path().join("dev");
    write_repo(&dev_repo);
    seed_bound_graph(&mut bound, &dev_repo);
    let registry = build_registry(&runtime);

    let dev_nodes_before = bound.graph.read().num_nodes();
    let dev_snapshot_before = std::fs::read(&bound.graph_path).expect("bound snapshot");
    let dev_roots_before = bound.ingest_roots.clone();

    // (a) a birth of an unrelated root leaves the dev graph alone.
    let stranger = temp.path().join("newborn");
    write_repo(&stranger);
    let receipt = ceremony_birth(&registry, &bound, &canonical(&stranger));
    assert_eq!(receipt["ok"], json!(true), "receipt was {receipt}");
    assert_eq!(bound.graph.read().num_nodes(), dev_nodes_before);
    assert_eq!(
        std::fs::read(&bound.graph_path).expect("bound snapshot"),
        dev_snapshot_before,
        "not one byte of the dev graph's snapshot may move during a birth"
    );
    assert_eq!(bound.ingest_roots, dev_roots_before);
    assert_eq!(receipt["dev_graph_untouched"], json!(true));

    // (b) a birth AT the dev graph's own root refuses: you are home already, and
    //     a second brain over the same repo would shadow the owner's own.
    let shadow = ceremony_birth(&registry, &bound, &canonical(&dev_repo));
    assert_eq!(shadow["ok"], json!(false), "payload was {shadow}");
    assert_eq!(shadow["refused"], json!("birth_root_is_bound_graph"));
    assert_eq!(bound.graph.read().num_nodes(), dev_nodes_before);
}

// ---------------------------------------------------------------------------
// §5.7 (f) — concurrent second birth refuses (single-flight)
// ---------------------------------------------------------------------------

/// Single-flight per canonical root (the cp32 TOCTOU requirement). A second
/// birth of a root already in flight refuses; it never queues behind the first
/// and measures "is the destination empty?" against a destination the first is
/// about to fill.
///
/// Driven by claiming the root directly rather than by racing two threads: the
/// property is "the claim is exclusive", and a race would test the scheduler.
#[test]
fn spec2_5_7f_concurrent_second_birth_of_a_root_in_flight_refuses() {
    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    let bound = build_bound_state(&runtime);
    let registry = build_registry(&runtime);
    let repo = temp.path().join("newborn");
    write_repo(&repo);
    let key = canonical(&repo);

    let held =
        brain_birth::claim_birth_root_for_test(&key).expect("an unclaimed root must be claimable");
    let refusal = ceremony_birth(&registry, &bound, &key);
    assert_eq!(refusal["ok"], json!(false), "payload was {refusal}");
    assert_eq!(refusal["refused"], json!("birth_in_flight"));
    assert!(
        !registry.store_dir_for(&key).exists(),
        "a refused birth must leave the destination untouched"
    );

    // Released on drop, including down a panicking path — the next birth runs.
    drop(held);
    let receipt = ceremony_birth(&registry, &bound, &key);
    assert_eq!(receipt["ok"], json!(true), "receipt was {receipt}");
}

// ---------------------------------------------------------------------------
// §5.7 (g) — a non-empty destination refuses, NAMING what occupies it
// ---------------------------------------------------------------------------

/// "Empty destination" is defined ON DISK (the cp32 requirement: no orphan
/// manifest, snapshot, or checkpoint). Each of those three, alone, must stop a
/// birth — and the refusal must NAME what it found, because a bare "not empty"
/// sends a human to guess at their own filesystem.
#[test]
fn spec2_5_7g_a_non_empty_destination_refuses_naming_what_occupies_it() {
    for occupant in [
        "project_brain.json",
        "graph_snapshot.json",
        "checkpoint-store/CURRENT",
    ] {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = temp.path().join("runtime");
        let bound = build_bound_state(&runtime);
        let registry = build_registry(&runtime);
        let repo = temp.path().join("newborn");
        write_repo(&repo);
        let key = canonical(&repo);

        // An ORPHAN artifact — the footprint of a brain that was, or of a
        // half-finished something. Either way the destination is not empty.
        let store = registry.store_dir_for(&key);
        let artifact = store.join(occupant);
        std::fs::create_dir_all(artifact.parent().expect("artifact parent")).expect("mk store");
        std::fs::write(&artifact, "{}\n").expect("write occupant");
        let before = std::fs::read(&artifact).expect("occupant bytes");

        let refusal = ceremony_birth(&registry, &bound, &key);

        assert_eq!(refusal["ok"], json!(false), "payload was {refusal}");
        assert_eq!(refusal["refused"], json!("birth_destination_not_empty"));
        let named = refusal["occupied_by"]
            .as_array()
            .unwrap_or_else(|| {
                panic!("the refusal must NAME what occupies the destination: {refusal}")
            })
            .iter()
            .filter_map(|value| value.as_str())
            .any(|entry| entry.contains(occupant.split('/').next_back().expect("occupant leaf")));
        assert!(named, "{occupant} was not named in {refusal}");
        assert_eq!(
            std::fs::read(&artifact).expect("occupant bytes"),
            before,
            "a refused birth may not touch what it found"
        );
    }
}

/// The migration/birth separation (§3: "Migration stays a boot-time fact with no
/// verb"). A destination holding a previous runtime is exactly the shape a
/// migration would adopt — and birth must refuse it rather than become a second,
/// weaker adoption path. The refusal says so, so a human reading it is not left
/// thinking birth is the tool for their existing brain.
#[test]
fn spec2_5_7g2_birth_is_not_the_migration_of_an_existing_brain() {
    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    let bound = build_bound_state(&runtime);
    let registry = build_registry(&runtime);
    let repo = temp.path().join("already-has-a-brain");
    write_repo(&repo);
    let key = canonical(&repo);

    // A brain that already exists, born the ordinary way.
    let first = ceremony_birth(&registry, &bound, &key);
    assert_eq!(first["ok"], json!(true), "receipt was {first}");
    let store = registry.store_dir_for(&key);
    let manifest_before = std::fs::read(store.join("project_brain.json")).expect("manifest");

    // A second ceremony over the SAME root is not a re-birth and not an adoption.
    let refusal = ceremony_birth(&registry, &bound, &key);
    assert_eq!(refusal["ok"], json!(false), "payload was {refusal}");
    assert_eq!(refusal["refused"], json!("birth_destination_not_empty"));
    assert!(
        refusal["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("migration"),
        "the refusal must separate migration from birth in words a human reads: {refusal}"
    );
    assert_eq!(
        std::fs::read(store.join("project_brain.json")).expect("manifest"),
        manifest_before,
        "the existing brain must survive a refused second birth byte-identically"
    );
}

// ---------------------------------------------------------------------------
// §5.7 (h) — overlap classes refuse; `allow_overlap` is forbidden
// ---------------------------------------------------------------------------

/// §2: "overlap classes refuse; no `allow_overlap` below sovereign". The
/// sovereign path does not carry the escape hatch either — a ceremony that
/// passes `allow_overlap:true` is refused on the FLAG, before any overlap is
/// even computed, so the hatch cannot be reached from here at all.
#[test]
fn spec2_5_7h_allow_overlap_is_forbidden_on_the_birth_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    let bound = build_bound_state(&runtime);
    let registry = build_registry(&runtime);
    let repo = temp.path().join("newborn");
    write_repo(&repo);
    let key = canonical(&repo);

    let mut request = BirthRequest::ceremony(&key, AGENT);
    request.allow_overlap = true;
    let refusal = brain_birth::run_birth(&registry, &bound, &request, HumanOrigin::Cli)
        .expect("the ceremony must answer");

    assert_eq!(refusal["ok"], json!(false), "payload was {refusal}");
    assert_eq!(refusal["refused"], json!("birth_allow_overlap_forbidden"));
    assert!(
        !registry.store_dir_for(&key).exists(),
        "a refused birth must create nothing"
    );
}

/// An overlapping root refuses and NAMES the brain it would have collided with —
/// the twin-brain trap the mint path already knows (a parent folder of a brained
/// repo, or a child of one). Birth inherits that guard rather than growing a
/// second, weaker copy of it.
#[test]
fn spec2_5_7h2_an_overlapping_root_refuses_naming_the_existing_brain() {
    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    let bound = build_bound_state(&runtime);
    let registry = build_registry(&runtime);

    let parent = temp.path().join("workspace");
    let child = parent.join("repo");
    write_repo(&child);
    let child_key = canonical(&child);

    let born = ceremony_birth(&registry, &bound, &child_key);
    assert_eq!(born["ok"], json!(true), "receipt was {born}");

    // The mother-folder trap: birthing the PARENT would re-ingest the child from
    // above and fragment its memories across two stores.
    write_repo(&parent);
    let parent_key = canonical(&parent);
    let refusal = ceremony_birth(&registry, &bound, &parent_key);

    assert_eq!(refusal["ok"], json!(false), "payload was {refusal}");
    assert_eq!(
        refusal["refused"],
        json!("birth_root_overlaps_existing_brain")
    );
    assert_eq!(refusal["overlap_class"], json!("parent"));
    assert_eq!(refusal["existing_brain_root"], json!(child_key));
    assert!(
        !registry.store_dir_for(&parent_key).exists(),
        "a refused birth must create nothing"
    );
}

/// A root that does not resolve on disk is refused before anything is created —
/// `canonical_key` falls back to the raw string for an unresolvable path, and a
/// birth into a string is a birth into nowhere.
#[test]
fn spec2_5_7i_an_unresolvable_root_refuses_and_creates_nothing() {
    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    let bound = build_bound_state(&runtime);
    let registry = build_registry(&runtime);
    let ghost = temp
        .path()
        .join("never-existed")
        .to_string_lossy()
        .to_string();

    let refusal = ceremony_birth(&registry, &bound, &ghost);

    assert_eq!(refusal["ok"], json!(false), "payload was {refusal}");
    assert_eq!(refusal["refused"], json!("birth_root_unresolvable"));
    assert!(
        !registry.base_dir().join("..").join("stray").exists(),
        "no stray artifacts"
    );
    assert!(
        !registry.store_dir_for(&ghost).exists(),
        "a refused birth must create nothing"
    );
}

// ---------------------------------------------------------------------------
// §5.8 (birth half) — whole-or-nothing
// ---------------------------------------------------------------------------

/// The birth's durable transition must be SINGLE, so a `kill -9` at any instant
/// lands on "no brain" or "a whole brain", never on a half-built store a later
/// boot would warm into.
///
/// WHAT THIS TEST EXECUTES: that every refusal path leaves the destination store
/// dir ABSENT, and that the success path leaves it complete — i.e. the
/// destination is only ever created by the single committing step. Whatever the
/// birth builds before that step must be built somewhere the routing seam cannot
/// see, so a crash cannot leave a brain-shaped thing in the brain-shaped place.
///
/// DECLARED BOUNDARY — the fault-injection half is NOT executed here and is not
/// claimed. Killing a real process mid-birth needs a live owner driven through
/// the ceremony ingress; that lives in
/// `m1nd-mcp/tests/spec2_birth_ceremony.rs`, which spawns the real binary. This
/// in-process test proves the SHAPE that makes the kill safe; that one proves
/// the kill.
#[test]
fn spec2_5_8_birth_creates_the_destination_in_one_committing_step() {
    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    let bound = build_bound_state(&runtime);
    let registry = build_registry(&runtime);
    let repo = temp.path().join("newborn");
    write_repo(&repo);
    let key = canonical(&repo);
    let store = registry.store_dir_for(&key);

    // Every refusal reason that can be reached before a commit: not one may
    // leave the destination existing.
    let ghost = temp.path().join("ghost").to_string_lossy().to_string();
    for refused_root in [ghost.as_str(), key.as_str()] {
        let payload = if refused_root == key {
            let mut request = BirthRequest::ceremony(refused_root, AGENT);
            request.allow_overlap = true;
            brain_birth::run_birth(&registry, &bound, &request, HumanOrigin::Cli)
                .expect("the ceremony must answer")
        } else {
            ceremony_birth(&registry, &bound, refused_root)
        };
        assert_eq!(payload["ok"], json!(false), "payload was {payload}");
        assert!(
            !store.exists(),
            "a refused birth left the destination existing at {store:?}"
        );
    }

    // The success path: the destination appears whole — the birth record AND the
    // first ingest, together, because they arrived together.
    let receipt = ceremony_birth(&registry, &bound, &key);
    assert_eq!(receipt["ok"], json!(true), "receipt was {receipt}");
    assert!(store.join("project_brain.json").is_file());
    assert!(store.join("graph_snapshot.json").is_file());

    // Nothing half-built survives beside the destination. A leftover staging
    // directory under the registry base is the footprint of a commit that was
    // not a single step.
    let leftovers: Vec<PathBuf> = std::fs::read_dir(registry.base_dir())
        .expect("read the registry base")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'))
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "left a partial birth behind: {leftovers:?}"
    );
}

// ---------------------------------------------------------------------------
// The floor and the catalog — the verb is named, gated, and NOT on SPEC-1's list
// ---------------------------------------------------------------------------

/// SPEC-2's admission, stated positively: the verb classifies to the action §2
/// names, at the `PositiveSovereign` floor §2 names, from `(tool, params)` alone.
#[test]
fn spec2_2a_the_birth_verb_classifies_at_positive_sovereign() {
    let classified = m1nd_mcp::action_routes::classify_mcp_action(
        BIRTH_TOOL,
        &plain_params("/tmp/anywhere"),
        m1nd_mcp::action_routes::TrustedMcpRouteFacts::default(),
    )
    .expect("the birth verb must classify purely from (tool, params)");
    assert_eq!(classified.action.as_str(), brain_birth::BIRTH_ACTION);
    assert_eq!(
        classified.authority_floor,
        m1nd_control::AuthorityFloor::PositiveSovereign,
        "§2: PositiveSovereign"
    );
}

/// SPEC-1's ratified A2-local opening holds EXACTLY one action. Birth is
/// sovereign and must never appear on it: that list was ratified for one door,
/// and a second entry there would open the sovereign frontier through a hole cut
/// for a freshness re-scan.
#[test]
fn spec2_2b_birth_is_never_on_the_a2_local_allowlist() {
    assert!(
        !m1nd_mcp::action_consumers::GENERIC_A2_LOCAL_ADMITTED_ACTIONS
            .contains(&brain_birth::BIRTH_ACTION),
        "the sovereign birth must not be admitted through SPEC-1's A2-local door"
    );
    assert_eq!(
        m1nd_mcp::action_consumers::GENERIC_A2_LOCAL_ADMITTED_ACTIONS,
        ["graph.ingest.refresh_declared_root"],
        "SPEC-1's list is ratified at exactly one action"
    );
}

/// The advertised surface may not sell what policy refuses. The verb carries the
/// house annotation and names its floor, from birth — the derived guard
/// (`public_surface_annotates_every_floor_gated_verb`) enforces it for the whole
/// registry, and this asserts it for THIS verb so the reason is legible here.
#[test]
fn spec2_2c_the_birth_verb_is_advertised_with_its_honest_annotation() {
    let registry = m1nd_mcp::server::all_tool_schemas();
    let tools = registry["tools"].as_array().expect("tools array");
    let birth = tools
        .iter()
        .find(|tool| tool["name"] == json!(BIRTH_TOOL))
        .unwrap_or_else(|| panic!("{BIRTH_TOOL} must be on the advertised registry"));
    let description = birth["description"].as_str().unwrap_or_default();

    assert!(
        description.contains("POLICY-DISABLED"),
        "the birth verb must carry the house marker: {description}"
    );
    assert!(
        description.contains("POSITIVE_SOVEREIGN"),
        "the annotation must name the floor an agent would have to satisfy: {description}"
    );
    // Never in the core menu: the ceremony is the human's, and the default agent
    // surface must not put it in front of one.
    assert!(
        !m1nd_mcp::server::core_menu_tool_names().contains(&BIRTH_TOOL),
        "the birth verb must not sit in the core agent-facing menu"
    );
    assert_eq!(
        birth["inputSchema"]["type"],
        json!("object"),
        "MCP spec: every inputSchema declares type=object at the TOP"
    );
}
