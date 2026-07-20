use crate as m1nd_mcp;

use std::fs::OpenOptions;
use std::io::Write;

use m1nd_control::{MissionState, MissionTransitionSource, OpaqueSignature};
use m1nd_core::{domain::DomainConfig, graph::Graph};
use m1nd_mcp::evidence_spine::{
    EvidenceAppendDisposition, EvidenceCausalAttachmentV1, EvidenceCorrelationLinkV1,
    EvidenceMissionBindingV1, EvidenceSpineIdentityV1, EvidenceSpineQueryV1, EvidenceSpineStore,
};
use m1nd_mcp::evidence_spine_owner;
use m1nd_mcp::mission_service::{
    EvidenceRefV1, MissionLetterV1, ReceiptCandidateV1, ReceiptImportAuditV1, ReceiptScopeV1,
    ReceiptV1, ReceiptValidityV1, EVIDENCE_REF_SCHEMA, MISSION_LETTER_V1_SCHEMA,
    RECEIPT_CANDIDATE_SCHEMA, RECEIPT_SCHEMA,
};
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use m1nd_mcp::system_blocks::ReceiptType;
use serde_json::{json, Value};

struct Fixture {
    identity: EvidenceSpineIdentityV1,
    binding: EvidenceMissionBindingV1,
    receipt: ReceiptV1,
    landed: MissionLetterV1,
    delegation: Value,
    mission_control: Value,
}

fn digest(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}

fn fixture(workspace: &std::path::Path) -> Fixture {
    let identity = EvidenceSpineIdentityV1::new("organism-1", "brain-1", workspace).unwrap();
    let binding = EvidenceMissionBindingV1::new(&identity, "mission-1", 1).unwrap();
    let source_head_id = format!("mlt:{}", digest('a'));

    let mut evidence = EvidenceRefV1 {
        schema: EVIDENCE_REF_SCHEMA.to_string(),
        kind: "test".to_string(),
        locator: "artifacts/g5-test.log".to_string(),
        sha256: digest('b'),
        producer_id: "runner-1".to_string(),
        command: Some(vec!["cargo".to_string(), "test".to_string()]),
        started_at: Some(10),
        ended_at: Some(20),
        retention_status: "retained".to_string(),
        evidence_digest: String::new(),
    };
    evidence.seal().unwrap();

    let mut candidate = ReceiptCandidateV1 {
        schema: RECEIPT_CANDIDATE_SCHEMA.to_string(),
        candidate_id: String::new(),
        brain_id: "brain-1".to_string(),
        mission_id: "mission-1".to_string(),
        mission_head_id: source_head_id.clone(),
        iteration_id: 1,
        block_id: "block-1".to_string(),
        store_version: 1,
        boundary_version: 1,
        contract_version: 1,
        execution_result_digest: digest('c'),
        receipt_type: ReceiptType::Test,
        evidence_refs: vec![evidence.clone()],
        synthetic: false,
        issuer: "runner-1".to_string(),
        key_id: "runner-key-1".to_string(),
        algorithm: "p256-sha256".to_string(),
        candidate_digest: String::new(),
        signature: OpaqueSignature::new("candidate-signature"),
    };
    candidate.seal().unwrap();

    let mut receipt = ReceiptV1 {
        schema: RECEIPT_SCHEMA.to_string(),
        receipt_id: String::new(),
        receipt_digest: String::new(),
        transaction_id: "tx-1".to_string(),
        brain_id: "brain-1".to_string(),
        mission_id: "mission-1".to_string(),
        mission_head_id: source_head_id.clone(),
        iteration_id: 1,
        candidate_digest: candidate.candidate_digest.clone(),
        receipt_type: ReceiptType::Test,
        scope: ReceiptScopeV1 {
            block_id: "block-1".to_string(),
            store_version: 1,
            boundary_version: 1,
            contract_version: 1,
            resolution_hash: digest('d'),
        },
        evidence_refs: vec![evidence],
        validity: ReceiptValidityV1 {
            valid: true,
            expires_at: None,
            stales_on: vec!["store_version".to_string()],
        },
        emitter: "mission-service".to_string(),
        import_audit: ReceiptImportAuditV1 {
            imported_by: "owner-1".to_string(),
            imported_at: 30,
            expected_store_version: 1,
            resulting_store_version: 2,
            authority_snapshot_digest: digest('e'),
        },
        issuer: "owner-1".to_string(),
        key_id: "owner-key-1".to_string(),
        algorithm: "p256-sha256".to_string(),
        signature: OpaqueSignature::new("receipt-signature"),
    };
    receipt.receipt_digest = receipt.compute_receipt_digest().unwrap();
    receipt.receipt_id = format!("rcp:{}", receipt.receipt_digest);

    let mut landed = MissionLetterV1 {
        schema: MISSION_LETTER_V1_SCHEMA.to_string(),
        head_id: String::new(),
        brain_id: "brain-1".to_string(),
        mission_id: "mission-1".to_string(),
        mission_seq: 8,
        previous_head_id: Some(source_head_id),
        state: MissionState::Landed,
        iteration_id: 1,
        packet_digest: digest('f'),
        block_id: "block-1".to_string(),
        store_version: 2,
        boundary_version: 1,
        contract_version: 1,
        source: MissionTransitionSource::MissionServiceDecision,
        source_digest: digest('1'),
        authored_by: "mission-service".to_string(),
        transaction_id: Some("tx-1".to_string()),
        execution_dispatch: None,
        execution_result_digest: Some(digest('2')),
        review_result_digest: Some(digest('3')),
        receipt_candidate: Some(candidate),
        committed_receipt_id: Some(receipt.receipt_id.clone()),
        created_at: 30,
    };
    landed.head_id = format!("mlt:{}", landed.compute_head_digest().unwrap());

    let delegation = json!({
        "schema": "m1nd-delegation-packet-v0",
        "delegation_id": "dlg_1_child",
        "created_ms": 1,
        "expires_ms": 2,
        "mission": {
            "task": "prove G5",
            "agent_id": "agent-child",
            "binding": {
                "workspace_root": identity.workspace_root.clone(),
                "trust_mode": "full_trust"
            },
            "tier": "project"
        },
        "staleness": {"graph_generation": 1},
        "context": {"anchors": [], "sufficiency": {"state": "sufficient"}},
        "known_static_dependents": {"expected_change": [], "dependents": []},
        "proof": {"suggested_shell": ["cargo test"]},
        "prompt_markdown": "rendering is excluded from the correlation digest",
        "status": "live"
    });
    let mission_control = json!({
        "schema": "m1nd-mission-control-state-v1",
        "mission_id": "msn_1_orchestrator",
        "agent_id": "agent-orchestrator",
        "repo": identity.workspace_root.clone(),
        "task": "prove G5",
        "mode": "architecture",
        "budget": "normal",
        "risk": "high",
        "route": "direct_proof",
        "phase": "verify",
        "status": "closed",
        "created_at_ms": 1,
        "updated_at_ms": 2,
        "events": [{"event_id": "evt-1", "outcome": "pass"}],
        "claims": [{"claim_id": "claim-1", "evidence_refs": ["test:g5"]}],
        "handoffs": [],
        "non_claims": []
    });

    Fixture {
        identity,
        binding,
        receipt,
        landed,
        delegation,
        mission_control,
    }
}

fn exact_attachment(fixture: &Fixture) -> EvidenceCausalAttachmentV1 {
    EvidenceCausalAttachmentV1 {
        mission_head_id: Some(fixture.receipt.mission_head_id.clone()),
        transaction_id: Some(fixture.receipt.transaction_id.clone()),
    }
}

fn record_complete(store: &mut EvidenceSpineStore, fixture: &Fixture) {
    store.record_receipt(&fixture.receipt, 100).unwrap();
    store.record_mission_letter(&fixture.landed, 101).unwrap();
    store
        .record_delegation_packet(
            &fixture.binding,
            exact_attachment(fixture),
            &fixture.delegation,
            102,
        )
        .unwrap();
    store
        .record_mission_control(
            &fixture.binding,
            exact_attachment(fixture),
            &fixture.mission_control,
            103,
        )
        .unwrap();
}

fn owner_state(runtime: &std::path::Path, workspace: &std::path::Path) -> SessionState {
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        ..McpConfig::default()
    };
    let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code()).unwrap();
    state.workspace_root = Some(
        std::fs::canonicalize(workspace)
            .unwrap()
            .to_string_lossy()
            .to_string(),
    );
    state.workspace_root_source = Some("g5_test_owner_selection".to_string());
    state
}

#[test]
fn complete_correlation_joins_all_four_authorities_without_owning_them() {
    let workspace = tempfile::tempdir().unwrap();
    let runtime = tempfile::tempdir().unwrap();
    let fixture = fixture(workspace.path());
    let mut store = EvidenceSpineStore::open(runtime.path(), fixture.identity.clone()).unwrap();
    record_complete(&mut store, &fixture);

    let result = store
        .query(&EvidenceSpineQueryV1 {
            transaction_id: Some("tx-1".to_string()),
            ..EvidenceSpineQueryV1::default()
        })
        .unwrap();
    assert_eq!(result.integrity, "hash_chain_verified_on_open_and_append");
    assert_eq!(result.verified_rows, 4);
    assert_eq!(result.correlations.len(), 1);
    let model = &result.correlations[0];
    assert!(model.landed_core_complete);
    assert!(model.delegation_exactly_bound);
    assert!(model.mission_control_exactly_bound);
    assert!(model.cross_surface_complete);
    assert!(model.gaps.is_empty());
    assert_eq!(
        model.receipt_id.as_deref(),
        Some(fixture.receipt.receipt_id.as_str())
    );
    assert_eq!(
        model.landed_head_id.as_deref(),
        Some(fixture.landed.head_id.as_str())
    );
    assert_eq!(model.delegation_ids, ["dlg_1_child"]);
    assert_eq!(model.mission_control_ids, ["msn_1_orchestrator"]);
    assert!(result
        .non_claims
        .iter()
        .any(|line| line.contains("read projection")));
}

#[test]
fn restart_rebuilds_the_same_read_model_from_the_verified_log() {
    let workspace = tempfile::tempdir().unwrap();
    let runtime = tempfile::tempdir().unwrap();
    let fixture = fixture(workspace.path());
    let before = {
        let mut store = EvidenceSpineStore::open(runtime.path(), fixture.identity.clone()).unwrap();
        record_complete(&mut store, &fixture);
        store.query(&EvidenceSpineQueryV1::default()).unwrap()
    };
    let reopened = EvidenceSpineStore::open(runtime.path(), fixture.identity.clone()).unwrap();
    let after = reopened.query(&EvidenceSpineQueryV1::default()).unwrap();
    assert_eq!(before.chain_head_digest, after.chain_head_digest);
    assert_eq!(before.correlations, after.correlations);
    assert_eq!(reopened.recovery_report().verified_rows, 4);
}

#[test]
fn exact_replay_is_one_shot_even_when_observed_later() {
    let workspace = tempfile::tempdir().unwrap();
    let runtime = tempfile::tempdir().unwrap();
    let fixture = fixture(workspace.path());
    let mut store = EvidenceSpineStore::open(runtime.path(), fixture.identity.clone()).unwrap();
    let first = store.record_receipt(&fixture.receipt, 100).unwrap();
    let replay = store.record_receipt(&fixture.receipt, 999).unwrap();
    assert_eq!(first.disposition, EvidenceAppendDisposition::Appended);
    assert_eq!(replay.disposition, EvidenceAppendDisposition::Replayed);
    assert_eq!(first.event_id, replay.event_id);
    assert_eq!(
        store
            .query(&EvidenceSpineQueryV1::default())
            .unwrap()
            .verified_rows,
        1
    );
}

#[test]
fn conflicting_landed_binding_is_refused_before_append() {
    let workspace = tempfile::tempdir().unwrap();
    let runtime = tempfile::tempdir().unwrap();
    let fixture = fixture(workspace.path());
    let mut store = EvidenceSpineStore::open(runtime.path(), fixture.identity.clone()).unwrap();
    store.record_receipt(&fixture.receipt, 100).unwrap();
    let mut forged = fixture.landed.clone();
    forged.transaction_id = Some("tx-forged".to_string());
    forged.head_id = String::new();
    forged.head_id = format!("mlt:{}", forged.compute_head_digest().unwrap());
    let error = store.record_mission_letter(&forged, 101).unwrap_err();
    assert_eq!(error.code(), "landed_correlation_mismatch");
    assert_eq!(
        store
            .query(&EvidenceSpineQueryV1::default())
            .unwrap()
            .verified_rows,
        1
    );
}

#[test]
fn complete_row_corruption_fails_closed_on_restart() {
    let workspace = tempfile::tempdir().unwrap();
    let runtime = tempfile::tempdir().unwrap();
    let fixture = fixture(workspace.path());
    let log_path = {
        let mut store = EvidenceSpineStore::open(runtime.path(), fixture.identity.clone()).unwrap();
        store.record_receipt(&fixture.receipt, 100).unwrap();
        store.log_path().to_path_buf()
    };
    let mut bytes = std::fs::read(&log_path).unwrap();
    let needle = b"\"row_digest\":\"";
    let start = bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap()
        + needle.len();
    bytes[start] = if bytes[start] == b'a' { b'b' } else { b'a' };
    std::fs::write(&log_path, bytes).unwrap();
    let error = EvidenceSpineStore::open(runtime.path(), fixture.identity.clone()).unwrap_err();
    assert_eq!(error.code(), "evidence_spine_corruption");
}

#[test]
fn torn_uncommitted_tail_is_truncated_and_reported() {
    let workspace = tempfile::tempdir().unwrap();
    let runtime = tempfile::tempdir().unwrap();
    let fixture = fixture(workspace.path());
    let log_path = {
        let mut store = EvidenceSpineStore::open(runtime.path(), fixture.identity.clone()).unwrap();
        store.record_receipt(&fixture.receipt, 100).unwrap();
        store.log_path().to_path_buf()
    };
    let mut file = OpenOptions::new().append(true).open(&log_path).unwrap();
    file.write_all(b"{\"torn\"").unwrap();
    file.sync_all().unwrap();
    drop(file);

    let reopened = EvidenceSpineStore::open(runtime.path(), fixture.identity.clone()).unwrap();
    assert_eq!(reopened.recovery_report().recovered_torn_tail_bytes, 7);
    assert_eq!(reopened.recovery_report().verified_rows, 1);
    assert!(std::fs::read(&log_path).unwrap().ends_with(b"\n"));
}

#[test]
fn delegation_and_mission_control_wrong_workspace_bindings_are_refused() {
    let workspace = tempfile::tempdir().unwrap();
    let other_workspace = tempfile::tempdir().unwrap();
    let runtime = tempfile::tempdir().unwrap();
    let fixture = fixture(workspace.path());
    let mut store = EvidenceSpineStore::open(runtime.path(), fixture.identity.clone()).unwrap();

    let mut wrong_packet = fixture.delegation.clone();
    wrong_packet["mission"]["binding"]["workspace_root"] =
        json!(other_workspace.path().to_str().unwrap());
    let packet_error = store
        .record_delegation_packet(
            &fixture.binding,
            EvidenceCausalAttachmentV1::default(),
            &wrong_packet,
            100,
        )
        .unwrap_err();
    assert_eq!(packet_error.code(), "wrong_workspace_binding");

    let mut wrong_control = fixture.mission_control.clone();
    wrong_control["repo"] = json!(other_workspace.path().to_str().unwrap());
    let control_error = store
        .record_mission_control(
            &fixture.binding,
            EvidenceCausalAttachmentV1::default(),
            &wrong_control,
            101,
        )
        .unwrap_err();
    assert_eq!(control_error.code(), "wrong_workspace_binding");
}

#[test]
fn persisted_store_refuses_a_different_organism_brain_or_workspace() {
    let workspace = tempfile::tempdir().unwrap();
    let other_workspace = tempfile::tempdir().unwrap();
    let runtime = tempfile::tempdir().unwrap();
    let fixture = fixture(workspace.path());
    {
        let _store = EvidenceSpineStore::open(runtime.path(), fixture.identity.clone()).unwrap();
    }
    let wrong =
        EvidenceSpineIdentityV1::new("organism-1", "brain-1", other_workspace.path()).unwrap();
    let error = EvidenceSpineStore::open(runtime.path(), wrong).unwrap_err();
    assert_eq!(error.code(), "evidence_spine_identity_mismatch");
}

#[test]
fn owner_query_is_read_only_reports_torn_tail_and_refuses_client_brain_selection() {
    let workspace = tempfile::tempdir().unwrap();
    let runtime = tempfile::tempdir().unwrap();
    let fixture = fixture(workspace.path());
    let spine_root = runtime.path().join("evidence-spine");
    let log_path = {
        let mut store = EvidenceSpineStore::open(&spine_root, fixture.identity.clone()).unwrap();
        store.record_receipt(&fixture.receipt, 100).unwrap();
        store.record_mission_letter(&fixture.landed, 101).unwrap();
        store.log_path().to_path_buf()
    };
    let mut file = OpenOptions::new().append(true).open(&log_path).unwrap();
    file.write_all(b"{\"uncommitted\"").unwrap();
    file.sync_all().unwrap();
    drop(file);
    let bytes_before = std::fs::read(&log_path).unwrap();
    let lock_dir = runtime.path().join("evidence-spine/.locks");
    let locks_before = std::fs::read_dir(&lock_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();

    let mut state = owner_state(runtime.path(), workspace.path());
    let response = dispatch_tool(
        &mut state,
        "evidence_query",
        &json!({"transaction_id": "tx-1"}),
    )
    .unwrap();
    assert_eq!(response["brain_id"], "brain-1");
    assert_eq!(response["workspace_root"], fixture.identity.workspace_root);
    assert_eq!(response["verified_rows"], 2);
    assert_eq!(response["observed_uncommitted_tail_bytes"], 14);
    assert_eq!(
        response["integrity"],
        "hash_chain_verified_committed_prefix_uncommitted_tail_observed"
    );
    assert_eq!(std::fs::read(&log_path).unwrap(), bytes_before);
    let locks_after = std::fs::read_dir(&lock_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(locks_after, locks_before);

    let error = dispatch_tool(
        &mut state,
        "evidence_query",
        &json!({"brain_id": "brain-forged"}),
    )
    .unwrap_err();
    assert!(error.to_string().contains("unknown field `brain_id`"));
}

#[test]
fn owner_link_requires_g3_anchor_and_projects_coordination_without_raw_authority() {
    let workspace = tempfile::tempdir().unwrap();
    let other_workspace = tempfile::tempdir().unwrap();
    let runtime = tempfile::tempdir().unwrap();
    let fixture = fixture(workspace.path());
    let spine_root = runtime.path().join("evidence-spine");
    {
        let mut store = EvidenceSpineStore::open(&spine_root, fixture.identity.clone()).unwrap();
        store.record_receipt(&fixture.receipt, 100).unwrap();
        store.record_mission_letter(&fixture.landed, 101).unwrap();
    }
    let mut state = owner_state(runtime.path(), workspace.path());
    let link = EvidenceCorrelationLinkV1::from_letter(&fixture.landed).unwrap();
    evidence_spine_owner::validate_link(&state, "test", &link).unwrap();

    let packet_projection = evidence_spine_owner::record_delegation_packet(
        &state,
        Some(&link),
        &fixture.delegation,
        102,
    );
    assert_eq!(packet_projection["status"], "appended");
    let outcome = json!({
        "schema": "m1nd-delegation-outcome-v0",
        "delegation_id": "dlg_1_child",
        "grader": "agent-orchestrator",
        "outcome": "success",
        "outcome_unverified": false,
        "graph_drifted": false,
        "touched_count": 1,
        "unpredicted": []
    });
    let outcome_projection =
        evidence_spine_owner::record_delegation_outcome(&state, Some(&link), &outcome, 103);
    assert_eq!(outcome_projection["status"], "appended");
    let control_projection = evidence_spine_owner::record_mission_control(
        &state,
        Some(&link),
        &fixture.mission_control,
        104,
    );
    assert_eq!(control_projection["status"], "appended");

    let response = dispatch_tool(
        &mut state,
        "evidence_query",
        &json!({"mission_id": "mission-1"}),
    )
    .unwrap();
    assert_eq!(response["verified_rows"], 5);
    assert_eq!(
        response["correlations"][0]["delegation_ids"],
        json!(["dlg_1_child"])
    );
    assert_eq!(
        response["correlations"][0]["mission_control_ids"],
        json!(["msn_1_orchestrator"])
    );

    let unanchored = EvidenceCorrelationLinkV1::new(
        "mission-1",
        1,
        format!("mlt:{}", digest('9')),
        Some("tx-1".to_string()),
    )
    .unwrap();
    let error = evidence_spine_owner::validate_link(&state, "test", &unanchored).unwrap_err();
    assert!(error.to_string().contains("evidence_binding_unanchored"));

    state.workspace_root = Some(
        std::fs::canonicalize(other_workspace.path())
            .unwrap()
            .to_string_lossy()
            .to_string(),
    );
    let error = dispatch_tool(&mut state, "evidence_query", &json!({})).unwrap_err();
    assert!(error.to_string().contains("wrong_workspace_binding"));
}

#[test]
fn mission_head_filter_matches_non_landed_and_landed_events_but_not_a_foreign_head() {
    let workspace = tempfile::tempdir().unwrap();
    let runtime = tempfile::tempdir().unwrap();
    let fixture = fixture(workspace.path());
    let mut non_landed = fixture.landed.clone();
    non_landed.head_id.clear();
    non_landed.mission_seq = 7;
    non_landed.previous_head_id = None;
    non_landed.state = MissionState::MergeWait;
    non_landed.store_version = 1;
    non_landed.transaction_id = None;
    non_landed.committed_receipt_id = None;
    non_landed.head_id = format!("mlt:{}", non_landed.compute_head_digest().unwrap());

    let mut store = EvidenceSpineStore::open(runtime.path(), fixture.identity.clone()).unwrap();
    store.record_mission_letter(&non_landed, 99).unwrap();
    store.record_receipt(&fixture.receipt, 100).unwrap();
    store.record_mission_letter(&fixture.landed, 101).unwrap();

    for head in [&non_landed.head_id, &fixture.landed.head_id] {
        let result = store
            .query(&EvidenceSpineQueryV1 {
                mission_head_id: Some(head.clone()),
                ..EvidenceSpineQueryV1::default()
            })
            .unwrap();
        assert_eq!(result.correlations.len(), 1, "canonical head {head}");
    }
    let foreign = store
        .query(&EvidenceSpineQueryV1 {
            mission_head_id: Some(format!("mlt:{}", digest('9'))),
            ..EvidenceSpineQueryV1::default()
        })
        .unwrap();
    assert!(foreign.correlations.is_empty());
}
