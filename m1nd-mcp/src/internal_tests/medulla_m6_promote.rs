//! MEDULLA ladder R4 / slice M6 — the `promote` verb + the C8 evidence riders.
//! The public Streamable-HTTP crossing is now intentionally fail-closed at
//! `POSITIVE_SOVEREIGN` until an exact typed G2 authority consumer exists. The
//! active integration test pins that refusal. Historical end-to-end crossing
//! cases remain ignored as executable acceptance criteria for that future
//! consumer; pure owner-internal logic remains covered in
//! `promote_handlers::tests`.
//!
//! THE LAWS UNDER TEST (MEDULLA-PRD §7, ORGANISM-PRD §C8.2/§C8.3):
//!   - a typed-authorized promote copies a VERIFIED project claim UP into the medulla with the full
//!     audit trail (Origin-Brain, Origin-Claim, Promoted-By, Promotion-Reason);
//!     the project witness stays in place stamped Promoted-To (promotion ELEVATES,
//!     never moves — MED-INV-3).
//!   - C8.2: a promoted claim's code evidence is origin-qualified (<root>#<path>)
//!     so freshness delegates to the origin brain — a medulla claim never reads
//!     fresher than it can prove.
//!   - C8.3 content eligibility follows authority; `State` and source labels do
//!     not themselves authorize promotion.
//!   - the promoted claim now surfaces CROSS-BRAIN in another brain's default beat
//!     (the R3 tier=medulla path), origin-labeled.
//!   - a weaker re-promotion bounces (WouldDowngrade); a secret is refused at the
//!     hygiene floor; demotion round-trips and never touches the witness.
//!
//! The frozen public surface deliberately prefers unavailable over forgeable.
#![cfg(feature = "serve")]

use crate as m1nd_mcp;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::http::HeaderMap;
use m1nd_mcp::brain_runtime::BrainSessionCell;
use serde_json::Value;
use tokio::sync::broadcast;

use m1nd_mcp::http_server::{AppState, SseEvent};
use m1nd_mcp::mcp_http::{handle_mcp_post, new_mcp_session_registry};
use m1nd_mcp::project_brains::ProjectBrainRegistry;
use m1nd_mcp::server::{tool_schemas, McpConfig, McpServer};

// ---------------------------------------------------------------------------
// Fixture — neutral names only (NO other-project names, NO personal paths).
// ---------------------------------------------------------------------------

fn write_repo(root: &Path, tag: &str) {
    std::fs::create_dir_all(root.join("src")).expect("mk repo src");
    std::fs::write(
        root.join("src/lib.rs"),
        format!("pub fn {tag}_probe() -> i64 {{ 42 }}\n"),
    )
    .expect("write lib.rs");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"fx\"\n").expect("write toml");
}

struct Owner {
    app: Arc<AppState>,
    runtime: PathBuf,
}

fn mk_owner(runtime: &Path) -> Owner {
    std::fs::create_dir_all(runtime).expect("mk runtime");
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        registry_dir: Some(runtime.join("registry")),
        ..Default::default()
    };
    let server = McpServer::new(config).expect("boot owner");
    let session = Arc::new(BrainSessionCell::new(server.into_session_state()));
    let (event_tx, _rx) = broadcast::channel::<SseEvent>(64);
    let tool_schemas_cache = tool_schemas()
        .get("tools")
        .cloned()
        .unwrap_or(Value::Array(vec![]));
    let project_brains = Arc::new(ProjectBrainRegistry::with_capacity(
        runtime.join("project-brains"),
        Some(runtime.join("registry")),
        4,
    ));
    Owner {
        app: Arc::new(AppState {
            session,
            tool_schemas_cache,
            event_tx,
            event_log_path: None,
            registry_dir: Some(runtime.join("registry")),
            mcp_sessions: new_mcp_session_registry(),
            project_brains,
            runnerd: Arc::new(m1nd_mcp::runnerd_owner::RunnerdRegistry::default()),
            ui_authority: Arc::new(m1nd_mcp::ui_attestation::UiBundleAttestor::default()),
            mission_service: None,
            external_mutation_service: None,
            authority_service: None,
            autonomy_owner: None,
        }),
        runtime: runtime.to_path_buf(),
    }
}

impl Owner {
    async fn post(
        &self,
        session: Option<&str>,
        caller_root: Option<&Path>,
        body: Value,
    ) -> (Value, Option<String>) {
        let mut headers = HeaderMap::new();
        if let Some(sid) = session {
            headers.insert("mcp-session-id", sid.parse().unwrap());
        }
        if let Some(root) = caller_root {
            headers.insert("m1nd-caller-root", root.to_string_lossy().parse().unwrap());
        }
        let resp = handle_mcp_post(
            axum::extract::State(self.app.clone()),
            headers,
            Bytes::from(body.to_string()),
        )
        .await;
        let minted = resp
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("read body");
        let parsed = serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null);
        (parsed, minted)
    }

    async fn init_session(&self, caller_root: &Path) -> String {
        let (_b, minted) = self
            .post(
                None,
                Some(caller_root),
                serde_json::json!({
                    "jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                        "clientInfo": {"name": "m6-probe", "version": "0"}}
                }),
            )
            .await;
        minted.expect("initialize mints a session id")
    }

    /// tools/call → (parsed JSON payload | Err(error message)).
    async fn tool_raw(
        &self,
        sid: &str,
        caller_root: &Path,
        name: &str,
        args: Value,
    ) -> Result<Value, String> {
        let (body, _) = self
            .post(
                Some(sid),
                Some(caller_root),
                serde_json::json!({
                    "jsonrpc": "2.0", "id": 7, "method": "tools/call",
                    "params": {"name": name, "arguments": args}
                }),
            )
            .await;
        let content = &body["result"]["content"][0]["text"];
        let text = content
            .as_str()
            .unwrap_or_else(|| panic!("tool {name} returned no content text: {body}"));
        if body["result"]["isError"].as_bool().unwrap_or(false) {
            return Err(text.to_string());
        }
        Ok(serde_json::from_str(text)
            .unwrap_or_else(|e| panic!("tool {name} content not JSON ({e}): {text}")))
    }

    async fn tool(&self, sid: &str, caller_root: &Path, name: &str, args: Value) -> Value {
        self.tool_raw(sid, caller_root, name, args)
            .await
            .unwrap_or_else(|e| panic!("tool {name} errored: {e}"))
    }

    async fn bootstrap(&self, root: &Path, agent: &str) -> String {
        let sid = self.init_session(root).await;
        let boot = self
            .tool(
                &sid,
                root,
                "ingest",
                serde_json::json!({
                    "path": root.to_string_lossy(),
                    "project_root": root.to_string_lossy(),
                    "agent_id": agent
                }),
            )
            .await;
        assert!(
            boot["ingest"]["node_count"].as_u64().unwrap_or(0) > 0,
            "bootstrap must ingest: {boot}"
        );
        sid
    }

    /// memorize a claim with an explicit State + evidence into the routed store.
    #[allow(clippy::too_many_arguments)] // a test helper: each arg is a distinct fixture knob
    async fn memorize_verified(
        &self,
        sid: &str,
        root: &Path,
        agent: &str,
        label: &str,
        text: &str,
        state: &str,
        evidence: Vec<&str>,
    ) -> Value {
        let ev: Vec<Value> = evidence.into_iter().map(Value::from).collect();
        let out = self
            .tool(
                sid,
                root,
                "memorize",
                serde_json::json!({
                    "agent_id": agent,
                    "node_label": label,
                    "state": state,
                    "claims": [{"label": label, "text": text, "confidence": "0.9", "evidence": ev}]
                }),
            )
            .await;
        assert!(out["refused"].is_null(), "memorize {label} refused: {out}");
        out
    }

    /// The medulla store dir (the bound owner's agent-memory).
    fn medulla_dir(&self) -> PathBuf {
        self.runtime.join("agent-memory")
    }
}

fn north_memory(north: &Value) -> Vec<Value> {
    north["memory"].as_array().cloned().unwrap_or_default()
}
fn any_claim_mentions(rows: &[Value], needle: &str) -> bool {
    rows.iter().any(|r| {
        r["claim"].as_str().unwrap_or("").contains(needle)
            || r["label"].as_str().unwrap_or("").contains(needle)
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_promote_is_sovereign_frozen_without_typed_g2_consumer() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner(&tmp.path().join("runtime"));
    let root_x = tmp.path().join("repo-x");
    write_repo(&root_x, "X");
    let sid = owner.init_session(&root_x).await;

    let error = owner
        .tool_raw(
            &sid,
            &root_x,
            "promote",
            serde_json::json!({
                "agent_id": "self-declared:orchestrator",
                "brain": root_x.to_string_lossy(),
                "claim": "ForgedVerifiedClaim",
                "reason": "self-attested metadata must not authorize a crossing"
            }),
        )
        .await
        .expect_err("public promote must fail closed without typed G2 authority");

    let normalized = error.to_ascii_lowercase();
    assert!(
        normalized.contains("positive_sovereign")
            || normalized.contains("positive sovereign")
            || normalized.contains("sovereign"),
        "refusal must name the sovereign authority boundary: {error}"
    );
    assert!(
        !owner.medulla_dir().exists()
            || std::fs::read_dir(owner.medulla_dir())
                .expect("read medulla dir")
                .next()
                .is_none(),
        "refused public promotion must not write a medulla artifact"
    );
}

// ===========================================================================
// (1) GREEN — promote copies a verified claim UP with the full audit trail, and
//     it surfaces CROSS-BRAIN in another brain's default beat (R3 tier=medulla).
//     This is the promote-then-recall + provenance-chain battery case.
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the future exact typed G2 promotion capability consumer"]
async fn promote_lands_audited_and_surfaces_cross_brain() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner(&tmp.path().join("runtime"));

    let root_x = tmp.path().join("repo-x");
    let root_y = tmp.path().join("repo-y");
    write_repo(&root_x, "X");
    write_repo(&root_y, "Y");
    let sid_x = owner.bootstrap(&root_x, "codex:maker").await;
    let sid_y = owner.bootstrap(&root_y, "agent-y").await;

    // A VERIFIED cross-project finding in brain X, anchored to code evidence.
    let claim = "TransversalRetryDoctrine";
    owner
        .memorize_verified(
            &sid_x,
            &root_x,
            "codex:maker",
            claim,
            "retry with jitter is safe across every service",
            "verified",
            vec!["src/lib.rs"],
        )
        .await;

    // --- RED sanity: before promotion, brain Y's default beat does NOT see it. ---
    let north_y_before = owner
        .tool(
            &sid_y,
            &root_y,
            "north",
            serde_json::json!({"agent_id": "agent-y", "task": format!("find {claim}")}),
        )
        .await;
    assert!(
        !any_claim_mentions(&north_memory(&north_y_before), claim),
        "pre-promotion: X's project claim must NOT be in Y's default beat"
    );

    // --- PROMOTE (orchestrator act) ---
    let promote = owner
        .tool(
            &sid_x,
            &root_x,
            "promote",
            serde_json::json!({
                "agent_id": "claude:orchestrator",
                "brain": root_x.to_string_lossy(),
                "claim": claim,
                "reason": "confirmed transversal — holds in every service, not one repo"
            }),
        )
        .await;
    assert_eq!(promote["ok"], true, "promote must succeed: {promote}");
    assert_eq!(promote["promoted"], true);
    // origin_brain is the claim's own Origin-Brain stamp (canonicalized at write
    // time by M5a) — assert it names repo-x, tolerant of /var vs /private/var
    // symlink resolution the tempdir introduces.
    let origin = promote["origin_brain"].as_str().unwrap_or("");
    assert!(
        origin.ends_with("repo-x"),
        "origin_brain must name the source brain: {origin}"
    );
    assert_eq!(promote["promoted_by"], "claude:orchestrator");
    // C8.2: this claim carried code evidence → origin-qualified (channel a).
    assert_eq!(
        promote["evidence"]["origin_qualified"], true,
        "code evidence must origin-qualify: {promote}"
    );
    assert_eq!(promote["evidence"]["evidence_unverifiable"], false);

    // --- THE AUDIT TRAIL ON DISK (the readable promotion chain) ---
    let medulla_file = owner
        .medulla_dir()
        .join("transversalretrydoctrine.light.md");
    let med_text = std::fs::read_to_string(&medulla_file).expect("medulla copy on disk");
    assert!(
        med_text.contains("Origin-Brain: "),
        "medulla copy carries Origin-Brain: {med_text}"
    );
    assert!(
        med_text.contains(&format!("Origin-Claim: {}", "transversalretrydoctrine")),
        "Origin-Claim: {med_text}"
    );
    assert!(
        med_text.contains("Promoted-By: claude:orchestrator"),
        "Promoted-By: {med_text}"
    );
    assert!(
        med_text.contains("Promotion-Reason: confirmed transversal"),
        "Promotion-Reason: {med_text}"
    );
    // C8.2 origin-qualified evidence: <origin_root>#<path>. The origin root is the
    // claim's canonicalized Origin-Brain stamp, so assert the shape (#src/lib.rs
    // qualified by a root ending in repo-x) rather than an exact raw-path prefix.
    let ev_line = med_text
        .lines()
        .find(|l| l.contains("evidence:") && l.contains("#src/lib.rs"))
        .unwrap_or_else(|| panic!("no origin-qualified evidence line: {med_text}"));
    assert!(
        ev_line.contains("repo-x#src/lib.rs"),
        "evidence must be origin-qualified (<origin_root>#src/lib.rs): {ev_line}"
    );

    // --- THE WITNESS stays in place, stamped Promoted-To (elevate, never move) ---
    let witness_dir = owner
        .app
        .project_brains
        .store_dir_for(&ProjectBrainRegistry::canonical_key(
            &root_x.to_string_lossy(),
        ));
    let witness_file = witness_dir
        .join("agent-memory")
        .join("transversalretrydoctrine.light.md");
    let wit_text = std::fs::read_to_string(&witness_file).expect("witness still on disk");
    assert!(
        wit_text.contains("Promoted-To: medulla@"),
        "witness must be stamped Promoted-To: {wit_text}"
    );

    // --- CROSS-BRAIN: brain Y's DEFAULT beat now carries it, tier: medulla. ---
    let north_y_after = owner
        .tool(
            &sid_y,
            &root_y,
            "north",
            serde_json::json!({"agent_id": "agent-y", "task": format!("find {claim}")}),
        )
        .await;
    let mem_y = north_memory(&north_y_after);
    assert!(
        any_claim_mentions(&mem_y, claim),
        "MED-INV-1 downstream: the PROMOTED claim MUST surface in Y's default beat: {mem_y:?}"
    );
    // Origin labeling: the surfaced row is tier=medulla (doctrine), not a leak.
    let row = mem_y.iter().find(|r| {
        r["claim"].as_str().unwrap_or("").contains(claim)
            || r["label"].as_str().unwrap_or("").contains(claim)
    });
    if let Some(row) = row {
        if let Some(tier) = row["tier"].as_str() {
            assert_eq!(
                tier, "medulla",
                "the promoted claim must be labeled tier: medulla, got {tier}"
            );
        }
    }
}

// ===========================================================================
// (2) C8.3 — the promotion evidence-class gate: an UNVERIFIED maker claim is
//     refused INSIDE the verb.
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the future exact typed G2 promotion capability consumer"]
async fn c83_refuses_unverified_claim() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner(&tmp.path().join("runtime"));
    let root_x = tmp.path().join("repo-x");
    write_repo(&root_x, "X");
    let sid_x = owner.bootstrap(&root_x, "codex:maker").await;

    // An AUTHORED (not verified), non-founder claim.
    owner
        .memorize_verified(
            &sid_x,
            &root_x,
            "codex:maker",
            "UnverifiedHunch",
            "maybe true",
            "authored",
            vec![],
        )
        .await;

    let err = owner
        .tool_raw(
            &sid_x,
            &root_x,
            "promote",
            serde_json::json!({
                "agent_id": "claude:orchestrator",
                "brain": root_x.to_string_lossy(),
                "claim": "UnverifiedHunch",
                "reason": "trying to promote an unverified claim"
            }),
        )
        .await;
    let msg = err.expect_err("an unverified claim must be REFUSED by C8.3");
    assert!(
        msg.contains("C8.3"),
        "refusal must cite the C8.3 gate: {msg}"
    );

    // And NOTHING was written into the medulla.
    assert!(
        !owner
            .medulla_dir()
            .join("unverifiedhunch.light.md")
            .exists(),
        "a C8.3-refused claim must not touch the medulla"
    );
}

// ===========================================================================
// (3) The downgrade bounce — a weaker re-promotion of a live medulla claim is
//     refused (the shared doctrine keeps its strongest form).
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the future exact typed G2 promotion capability consumer"]
async fn weaker_re_promotion_bounces() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner(&tmp.path().join("runtime"));
    let root_x = tmp.path().join("repo-x");
    let root_y = tmp.path().join("repo-y");
    write_repo(&root_x, "X");
    write_repo(&root_y, "Y");
    let sid_x = owner.bootstrap(&root_x, "agent-x").await;
    let sid_y = owner.bootstrap(&root_y, "agent-y").await;

    // Strong claim in X (verified, 0.9), promote it.
    owner
        .memorize_verified(
            &sid_x,
            &root_x,
            "agent-x",
            "SharedLaw",
            "the strong form",
            "verified",
            vec![],
        )
        .await;
    let ok = owner
        .tool(
            &sid_x,
            &root_x,
            "promote",
            serde_json::json!({
                "agent_id": "claude:orchestrator", "brain": root_x.to_string_lossy(),
                "claim": "SharedLaw", "reason": "strong"
            }),
        )
        .await;
    assert_eq!(ok["promoted"], true);

    // A WEAKER same-slug claim in Y (verified but 0.5), try to re-promote → bounce.
    let out = owner
        .tool(
            &sid_y,
            &root_y,
            "memorize",
            serde_json::json!({
                "agent_id": "agent-y", "node_label": "SharedLaw", "state": "verified",
                "claims": [{"label": "SharedLaw", "text": "a weaker form", "confidence": "0.5"}]
            }),
        )
        .await;
    assert!(
        out["refused"].is_null(),
        "seed memorize should land in Y: {out}"
    );

    let err = owner
        .tool_raw(
            &sid_y,
            &root_y,
            "promote",
            serde_json::json!({
                "agent_id": "claude:orchestrator", "brain": root_y.to_string_lossy(),
                "claim": "SharedLaw", "reason": "weaker re-promotion"
            }),
        )
        .await;
    let msg = err.expect_err("a weaker re-promotion must bounce as WouldDowngrade");
    assert!(
        msg.to_lowercase().contains("downgrade") || msg.contains("stronger"),
        "bounce reason: {msg}"
    );
}

// ===========================================================================
// (4) The hygiene floor — a secret in the claim text is refused at the door.
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the future exact typed G2 promotion capability consumer"]
async fn secret_in_claim_refused_at_hygiene_floor() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner(&tmp.path().join("runtime"));
    let root_x = tmp.path().join("repo-x");
    write_repo(&root_x, "X");
    let sid_x = owner.bootstrap(&root_x, "agent-x").await;

    owner
        .memorize_verified(
            &sid_x,
            &root_x,
            "agent-x",
            "LeakyClaim",
            "the deploy token is ghp_AAAABBBBCCCCDDDDEEEE1234567890",
            "verified",
            vec![],
        )
        .await;

    let err = owner
        .tool_raw(
            &sid_x,
            &root_x,
            "promote",
            serde_json::json!({
                "agent_id": "claude:orchestrator", "brain": root_x.to_string_lossy(),
                "claim": "LeakyClaim", "reason": "should be refused"
            }),
        )
        .await;
    let msg = err.expect_err("a secret in the claim must be refused at the hygiene floor");
    assert!(
        msg.contains("hygiene"),
        "refusal must cite the hygiene floor: {msg}"
    );
    assert!(
        !owner.medulla_dir().join("leakyclaim.light.md").exists(),
        "a hygiene-refused claim must never reach the medulla"
    );
}

// ===========================================================================
// (5) Demotion preserves the witness — un-sharing (a superseding medulla
//     memorize) NEVER destroys the local truth. The witness outlives it.
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the future exact typed G2 promotion capability consumer"]
async fn demotion_preserves_the_witness() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner(&tmp.path().join("runtime"));
    let root_x = tmp.path().join("repo-x");
    write_repo(&root_x, "X");
    let sid_x = owner.bootstrap(&root_x, "agent-x").await;

    owner
        .memorize_verified(
            &sid_x,
            &root_x,
            "agent-x",
            "MaybeTransversal",
            "looked cross-project",
            "verified",
            vec![],
        )
        .await;
    owner
        .tool(
            &sid_x,
            &root_x,
            "promote",
            serde_json::json!({
                "agent_id": "claude:orchestrator", "brain": root_x.to_string_lossy(),
                "claim": "MaybeTransversal", "reason": "seemed cross-project"
            }),
        )
        .await;

    let witness_dir = owner
        .app
        .project_brains
        .store_dir_for(&ProjectBrainRegistry::canonical_key(
            &root_x.to_string_lossy(),
        ));
    let witness_file = witness_dir
        .join("agent-memory")
        .join("maybetransversal.light.md");
    let witness_before = std::fs::read_to_string(&witness_file).expect("witness on disk");

    // DEMOTION: a superseding medulla memorize (moved_to back-pointer) — un-share.
    // Direct owner session (no caller root → the medulla store itself).
    let sid_owner = owner.init_session(&owner.runtime).await;
    let _demote = owner
        .tool(
            &sid_owner,
            &owner.runtime,
            "memorize",
            serde_json::json!({
                "agent_id": "claude:orchestrator", "node_label": "MaybeTransversal", "state": "verified",
                "claims": [{"label": "MaybeTransversal", "text": "moved_to: repo-x — turned out to be one repo's quirk", "confidence": "0.95"}]
            }),
        )
        .await;

    // THE WITNESS IS UNTOUCHED by the demotion — un-sharing never destroys local truth.
    let witness_after = std::fs::read_to_string(&witness_file).expect("witness STILL on disk");
    assert_eq!(
        witness_before, witness_after,
        "demotion must NOT touch the project witness (un-share, never destroy — MED-INV-3)"
    );
    assert!(
        witness_after.contains("Promoted-To: medulla@"),
        "witness keeps its Promoted-To stamp"
    );
}

// ===========================================================================
// (6) Unknown slug — a hard error, never a guess.
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the future exact typed G2 promotion capability consumer"]
async fn unknown_slug_hard_errors() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner(&tmp.path().join("runtime"));
    let root_x = tmp.path().join("repo-x");
    write_repo(&root_x, "X");
    let sid_x = owner.bootstrap(&root_x, "agent-x").await;

    let err = owner
        .tool_raw(
            &sid_x,
            &root_x,
            "promote",
            serde_json::json!({
                "agent_id": "claude:orchestrator", "brain": root_x.to_string_lossy(),
                "claim": "NoSuchClaimAnywhere", "reason": "there is nothing here"
            }),
        )
        .await;
    let msg = err.expect_err("an unknown slug must hard-error, never guess");
    assert!(
        msg.contains("nothing to promote") || msg.contains("no claim"),
        "error must name the miss: {msg}"
    );
}
