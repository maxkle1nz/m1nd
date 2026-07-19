use m1nd_control::{ExecutionDispatchState, MissionState, Role};
use serde_json::json;

use super::execution_dispatch::{
    ExecutionMissionHeadV1, RunnerClaimOutcome, RunnerExecutionInbox, RunnerInboxState,
    EXECUTION_MISSION_HEAD_SCHEMA,
};
use super::mission_service::*;
use super::mission_service_tests::{
    advance_to_merge_wait, authority, build_land, dispatch, execution_ack, hash, open_service,
    payload, service_decision, successful_result_and_candidate, transition_intent, BRAIN, MISSION,
    NOW, RUNNER, SERVICE_ACTOR,
};
use super::mission_service_transport::*;

fn transport_schema() -> String {
    MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA.to_string()
}

#[test]
fn h4nd_land_intent_cross_source_digest_vectors_are_stable() {
    let request = ExternalMissionServiceRequestV1::LandIntent {
        schema: transport_schema(),
        request_id: "correlation-is-excluded".to_string(),
        mission_id: "mission-1".to_string(),
        expected_head_id: "head-1".to_string(),
        candidate_id: "candidate-1".to_string(),
        expected_candidate_digest: "a".repeat(64),
        expected_store_version: 7,
        idempotency_key: "land-idem-1".to_string(),
    };
    assert_eq!(
        request.authority_object_digest().unwrap(),
        "e9d3f0d445d682cb05353e75d9ee013d936a7e458cdf91dbd36a879dde248a54"
    );

    let intent = LandIntentCoreV1 {
        schema: LAND_INTENT_CORE_SCHEMA.to_string(),
        brain_id: "/workspace/m1nd".to_string(),
        mission_id: "mission-1".to_string(),
        expected_head_id: "head-1".to_string(),
        candidate_id: "candidate-1".to_string(),
        expected_candidate_digest: "a".repeat(64),
        block_id: "sb_m1nd_core".to_string(),
        expected_store_version: 7,
        expected_boundary_version: 3,
        expected_contract_version: 2,
        resolution_hash: "b".repeat(64),
        idempotency_key: "land-idem-1".to_string(),
    };
    assert_eq!(
        intent.compute_intent_digest().unwrap(),
        "70586f404444c71b76f2ab3815c2623381310b99a0dcd2a3d1eed7d35a0f6818"
    );
}

fn transition_outcome(response: MissionServiceTransportResponseV1) -> TransitionOutcomeV1 {
    match response.result {
        MissionServiceTransportResultV1::MissionTransition { outcome } => *outcome,
        other => panic!("expected mission transition response, got {other:?}"),
    }
}

#[test]
fn raw_mission_bound_mutations_are_refused_before_capability_or_payload_parsing() {
    let directory = tempfile::tempdir().unwrap();
    let mut service = open_service(&directory);
    let initial_version = service.state_version();
    let privileged = authority(SERVICE_ACTOR, Role::MissionService, "cap-any", &hash('a'));

    for action in ["receipt_import", "landed", "mission_post"] {
        let body = serde_json::to_vec(&json!({
            "action": action,
            "authority": privileged,
            "capability": "cap-any",
            "deliberately_untyped_payload": {"state": "landed"}
        }))
        .unwrap();
        for context in [None, Some(&privileged)] {
            let error =
                dispatch_external_mission_json(&mut service, context, &body, NOW).unwrap_err();
            assert_eq!(error.code(), "legacy_direct_mutation_refused");
            assert!(error.to_string().contains(action));
        }
    }
    assert_eq!(service.state_version(), initial_version);
    assert!(service.head(MISSION).is_none());
    assert!(service.receipts().is_empty());
}

#[test]
fn typed_transition_cannot_publish_landed_or_bypass_execution_snapshot_routes() {
    let directory = tempfile::tempdir().unwrap();
    let mut service = open_service(&directory);
    let packet = hash('3');
    let transition_id = "transport-dispatch-guard";
    let dispatch = dispatch(transition_id, &packet);
    let transition_payload = payload(MissionTransitionEvidenceV1::MissionServiceDecision {
        decision: service_decision("transport-decision-guard"),
        dispatch: Some(dispatch),
    });
    let intent = transition_intent(
        &service,
        transition_id,
        MissionState::Dispatching,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-transport-guard",
        &transition_payload,
        packet,
        "idem-transport-guard",
    );
    let auth = authority(
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-transport-guard",
        &intent.intent_digest,
    );

    let bypass = ExternalMissionServiceRequestV1::MissionTransition {
        schema: transport_schema(),
        request_id: "request-dispatch-bypass".to_string(),
        intent: intent.clone(),
        payload: transition_payload.clone(),
    };
    let error =
        dispatch_external_mission_request(&mut service, Some(&auth), &bypass, NOW).unwrap_err();
    assert_eq!(error.code(), "execution_lifecycle_route_required");
    assert!(service.head(MISSION).is_none());

    let mut raw_landed_intent = intent;
    raw_landed_intent.to_state = MissionState::Landed;
    raw_landed_intent.seal().unwrap();
    let landed_auth = authority(
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-transport-guard",
        &raw_landed_intent.intent_digest,
    );
    let raw_landed = ExternalMissionServiceRequestV1::MissionTransition {
        schema: transport_schema(),
        request_id: "request-raw-landed".to_string(),
        intent: raw_landed_intent,
        payload: transition_payload,
    };
    let error =
        dispatch_external_mission_request(&mut service, Some(&landed_auth), &raw_landed, NOW)
            .unwrap_err();
    assert_eq!(error.code(), "legacy_direct_mutation_refused");
    assert!(service.head(MISSION).is_none());
}

#[test]
fn execution_transport_preserves_exact_head_snapshot_and_idempotency_bindings() {
    let directory = tempfile::tempdir().unwrap();
    let mut service = open_service(&directory);
    let packet = hash('3');
    let transition_id = "transport-dispatch";
    let dispatch = dispatch(transition_id, &packet);
    let dispatch_payload = payload(MissionTransitionEvidenceV1::MissionServiceDecision {
        decision: service_decision("transport-decision-dispatch"),
        dispatch: Some(dispatch.clone()),
    });
    let dispatch_intent = transition_intent(
        &service,
        transition_id,
        MissionState::Dispatching,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-transport-dispatch",
        &dispatch_payload,
        packet.clone(),
        "idem-transport-dispatch",
    );
    let dispatch_authority = authority(
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-transport-dispatch",
        &dispatch_intent.intent_digest,
    );
    let request = ExternalMissionServiceRequestV1::ExecutionDispatch {
        schema: transport_schema(),
        request_id: "request-execution-dispatch".to_string(),
        intent: dispatch_intent,
        payload: dispatch_payload,
    };
    let body = serde_json::to_vec(&request).unwrap();
    let first = transition_outcome(
        dispatch_external_mission_json(&mut service, Some(&dispatch_authority), &body, NOW)
            .unwrap(),
    );
    assert!(!first.deduplicated);
    assert_eq!(first.letter.state, MissionState::Dispatching);
    assert_eq!(first.letter.execution_dispatch.as_ref(), Some(&dispatch));
    let mut replay_request = request;
    if let ExternalMissionServiceRequestV1::ExecutionDispatch { request_id, .. } =
        &mut replay_request
    {
        *request_id = "request-execution-dispatch-retry".to_string();
    }
    let replay_response = dispatch_external_mission_json(
        &mut service,
        Some(&dispatch_authority),
        &serde_json::to_vec(&replay_request).unwrap(),
        NOW,
    )
    .unwrap();
    assert_eq!(
        replay_response.request_id,
        "request-execution-dispatch-retry"
    );
    let replay = transition_outcome(replay_response);
    assert!(replay.deduplicated);
    assert_eq!(replay.letter.head_id, first.letter.head_id);
    assert_eq!(service.head(MISSION).unwrap().mission_seq, 1);
    assert_eq!(
        service
            .execution_dispatch(&dispatch.execution_id)
            .unwrap()
            .state,
        ExecutionDispatchState::Intent
    );

    let runner_path = directory.path().join("transport-runner-inbox.jsonl");
    let mut runner = RunnerExecutionInbox::open(&runner_path, RUNNER).unwrap();
    let claim = match runner.claim_for_spawn(dispatch.clone(), NOW - 45).unwrap() {
        RunnerClaimOutcome::Spawn(permit) => permit.claim,
        RunnerClaimOutcome::AlreadyClaimed { .. } => panic!("first delivery must spawn once"),
    };
    runner
        .mark_process_started(
            &dispatch.execution_id,
            &claim.claim_id,
            "pid:71:start:900",
            NOW - 40,
        )
        .unwrap();
    let started_snapshot = runner.get(&dispatch.execution_id).unwrap().clone();
    assert_eq!(started_snapshot.state, RunnerInboxState::Started);
    let ack = execution_ack(&dispatch);
    let ack_payload =
        payload(MissionTransitionEvidenceV1::ExecutionDispatchAck { ack: ack.clone() });
    let ack_intent = transition_intent(
        &service,
        "transport-started",
        MissionState::Executing,
        RUNNER,
        Role::Runner,
        "cap-transport-started",
        &ack_payload,
        packet.clone(),
        "idem-transport-started",
    );
    let ack_authority = authority(
        RUNNER,
        Role::Runner,
        "cap-transport-started",
        &ack_intent.intent_digest,
    );

    let mut stale_head_intent = ack_intent.clone();
    stale_head_intent.expected_head_id = Some("mlt:stale".to_string());
    stale_head_intent.seal().unwrap();
    let stale_head_authority = authority(
        RUNNER,
        Role::Runner,
        "cap-transport-started",
        &stale_head_intent.intent_digest,
    );
    let stale_head_request = ExternalMissionServiceRequestV1::ExecutionStarted {
        schema: transport_schema(),
        request_id: "request-stale-head".to_string(),
        snapshot: started_snapshot.clone(),
        intent: stale_head_intent,
        payload: ack_payload.clone(),
    };
    let error = dispatch_external_mission_request(
        &mut service,
        Some(&stale_head_authority),
        &stale_head_request,
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code(), "stale_head");
    assert_eq!(
        service.head(MISSION).unwrap().state,
        MissionState::Dispatching
    );

    let started_request = ExternalMissionServiceRequestV1::ExecutionStarted {
        schema: transport_schema(),
        request_id: "request-execution-started".to_string(),
        snapshot: started_snapshot,
        intent: ack_intent,
        payload: ack_payload,
    };
    let started_body = serde_json::to_vec(&started_request).unwrap();
    let executing = transition_outcome(
        dispatch_external_mission_json(&mut service, Some(&ack_authority), &started_body, NOW)
            .unwrap(),
    );
    assert_eq!(executing.letter.state, MissionState::Executing);
    let started_replay = transition_outcome(
        dispatch_external_mission_json(&mut service, Some(&ack_authority), &started_body, NOW)
            .unwrap(),
    );
    assert!(started_replay.deduplicated);
    assert_eq!(started_replay.letter.head_id, executing.letter.head_id);

    runner.record_ack(ack.clone(), NOW).unwrap();
    runner
        .observe_executing_transition(
            &dispatch.execution_id,
            &ack.ack_digest,
            ExecutionMissionHeadV1 {
                schema: EXECUTION_MISSION_HEAD_SCHEMA.to_string(),
                head_id: executing.letter.head_id.clone(),
                state: MissionState::Executing,
                iteration_id: executing.letter.iteration_id,
                packet_digest: executing.letter.packet_digest.clone(),
            },
            NOW,
        )
        .unwrap();
    let (result, candidate) = successful_result_and_candidate(&service, &dispatch);
    runner.record_result(result.clone(), NOW).unwrap();
    let terminal_snapshot = runner.get(&dispatch.execution_id).unwrap().clone();
    assert_eq!(terminal_snapshot.state, RunnerInboxState::Completed);
    let result_payload = payload(MissionTransitionEvidenceV1::ExecutionResult {
        result: result.clone(),
        candidate: Some(candidate.clone()),
    });
    let result_intent = transition_intent(
        &service,
        "transport-terminal",
        MissionState::Gate,
        RUNNER,
        Role::Runner,
        "cap-transport-terminal",
        &result_payload,
        packet,
        "idem-transport-terminal",
    );
    let result_authority = authority(
        RUNNER,
        Role::Runner,
        "cap-transport-terminal",
        &result_intent.intent_digest,
    );
    let terminal_request = ExternalMissionServiceRequestV1::ExecutionTerminal {
        schema: transport_schema(),
        request_id: "request-execution-terminal".to_string(),
        snapshot: terminal_snapshot,
        intent: result_intent,
        payload: result_payload,
    };
    let terminal_body = serde_json::to_vec(&terminal_request).unwrap();
    let gate = transition_outcome(
        dispatch_external_mission_json(&mut service, Some(&result_authority), &terminal_body, NOW)
            .unwrap(),
    );
    assert_eq!(gate.letter.state, MissionState::Gate);
    assert_eq!(
        gate.letter.execution_result_digest,
        Some(result.result_digest)
    );
    assert_eq!(gate.letter.receipt_candidate, Some(candidate));
    let terminal_replay = transition_outcome(
        dispatch_external_mission_json(&mut service, Some(&result_authority), &terminal_body, NOW)
            .unwrap(),
    );
    assert!(terminal_replay.deduplicated);
    assert_eq!(terminal_replay.letter.head_id, gate.letter.head_id);
    assert_eq!(service.head(MISSION).unwrap().mission_seq, 3);
}

#[test]
fn land_facade_rereads_canonical_bindings_and_replays_without_duplicate_effects() {
    let directory = tempfile::tempdir().unwrap();
    let mut service = open_service(&directory);
    let merge_wait = advance_to_merge_wait(&mut service, false);
    let candidate = merge_wait.receipt_candidate.as_ref().unwrap();

    let land_intent_request = ExternalMissionServiceRequestV1::LandIntent {
        schema: transport_schema(),
        request_id: "request-land-intent".to_string(),
        mission_id: MISSION.to_string(),
        expected_head_id: merge_wait.head_id.clone(),
        candidate_id: candidate.candidate_id.clone(),
        expected_candidate_digest: candidate.candidate_digest.clone(),
        expected_store_version: merge_wait.store_version,
        idempotency_key: "idem-transport-land".to_string(),
    };
    let read_authority = authority(
        "transport-reader",
        Role::Author,
        "cap-transport-land-intent",
        &land_intent_request.authority_object_digest().unwrap(),
    );
    let response = dispatch_external_mission_request(
        &mut service,
        Some(&read_authority),
        &land_intent_request,
        NOW,
    )
    .unwrap();
    let intent = match response.result {
        MissionServiceTransportResultV1::LandIntent { intent } => *intent,
        other => panic!("expected land intent, got {other:?}"),
    };
    assert_eq!(intent.brain_id, BRAIN);
    assert_eq!(intent.expected_head_id, merge_wait.head_id);
    assert_eq!(intent.candidate_id, candidate.candidate_id);
    assert_eq!(intent.expected_candidate_digest, candidate.candidate_digest);
    assert_eq!(intent.expected_store_version, merge_wait.store_version);
    assert_eq!(
        intent.expected_boundary_version,
        merge_wait.boundary_version
    );
    assert_eq!(
        intent.expected_contract_version,
        merge_wait.contract_version
    );

    let mut stale_intent_request = land_intent_request.clone();
    if let ExternalMissionServiceRequestV1::LandIntent {
        expected_store_version,
        ..
    } = &mut stale_intent_request
    {
        *expected_store_version += 1;
    }
    let stale_read_authority = authority(
        "transport-reader",
        Role::Author,
        "cap-transport-land-intent",
        &stale_intent_request.authority_object_digest().unwrap(),
    );
    let error = dispatch_external_mission_request(
        &mut service,
        Some(&stale_read_authority),
        &stale_intent_request,
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code(), "stale_store");
    assert!(service.receipts().is_empty());

    let (land_authority, land_request) = build_land(
        &service,
        "transport-land-transaction",
        "idem-transport-land",
    );
    let request = ExternalMissionServiceRequestV1::Land {
        schema: transport_schema(),
        request_id: "request-land".to_string(),
        request: land_request,
    };
    let mut injected = serde_json::to_value(&request).unwrap();
    injected
        .as_object_mut()
        .unwrap()
        .insert("receipt".to_string(), json!({"forged": true}));
    injected
        .as_object_mut()
        .unwrap()
        .insert("authority".to_string(), json!({"forged": true}));
    let error = dispatch_external_mission_json(
        &mut service,
        Some(&land_authority),
        &serde_json::to_vec(&injected).unwrap(),
        NOW,
    )
    .unwrap_err();
    assert_eq!(error.code(), "invalid_transport_request");
    assert!(service.receipts().is_empty());
    assert_eq!(
        service.head(MISSION).unwrap().state,
        MissionState::MergeWait
    );

    let body = serde_json::to_vec(&request).unwrap();
    let missing_authority =
        dispatch_external_mission_json(&mut service, None, &body, NOW).unwrap_err();
    assert_eq!(missing_authority.code(), "missing_authenticated_authority");
    assert!(service.receipts().is_empty());

    let first =
        dispatch_external_mission_json(&mut service, Some(&land_authority), &body, NOW).unwrap();
    let first = match first.result {
        MissionServiceTransportResultV1::Land { outcome } => *outcome,
        other => panic!("expected land outcome, got {other:?}"),
    };
    assert!(!first.deduplicated);
    assert_eq!(service.receipts().len(), 1);
    assert_eq!(service.head(MISSION).unwrap().state, MissionState::Landed);
    assert_eq!(
        service
            .head(MISSION)
            .unwrap()
            .committed_receipt_id
            .as_deref(),
        Some(first.receipt_id.as_str())
    );

    let replay =
        dispatch_external_mission_json(&mut service, Some(&land_authority), &body, NOW).unwrap();
    let replay = match replay.result {
        MissionServiceTransportResultV1::Land { outcome } => *outcome,
        other => panic!("expected land outcome, got {other:?}"),
    };
    assert!(replay.deduplicated);
    assert_eq!(replay.receipt_id, first.receipt_id);
    assert_eq!(replay.letter_id, first.letter_id);
    assert_eq!(service.receipts().len(), 1);
}

#[test]
fn transport_metadata_and_refusal_contract_are_strict_and_stable() {
    let directory = tempfile::tempdir().unwrap();
    let mut service = open_service(&directory);
    let packet = hash('3');
    let transition_id = "transport-metadata";
    let transition_payload = payload(MissionTransitionEvidenceV1::MissionServiceDecision {
        decision: service_decision("transport-metadata-decision"),
        dispatch: Some(dispatch(transition_id, &packet)),
    });
    let intent = transition_intent(
        &service,
        transition_id,
        MissionState::Dispatching,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-transport-metadata",
        &transition_payload,
        packet,
        "idem-transport-metadata",
    );
    let request = ExternalMissionServiceRequestV1::ExecutionDispatch {
        schema: "m1nd-mission-service-transport-request-v999".to_string(),
        request_id: "request-metadata".to_string(),
        intent,
        payload: transition_payload,
    };
    let error = dispatch_external_mission_request(&mut service, None, &request, NOW).unwrap_err();
    assert_eq!(error.code(), "transport_schema_mismatch");
    let refusal = error.to_refusal(Some(request.request_id()));
    assert_eq!(refusal.schema, MISSION_SERVICE_TRANSPORT_REFUSAL_SCHEMA);
    assert_eq!(refusal.request_id.as_deref(), Some("request-metadata"));
    assert_eq!(refusal.code, "transport_schema_mismatch");
    let round_trip: MissionServiceTransportRefusalV1 =
        serde_json::from_slice(&serde_json::to_vec(&refusal).unwrap()).unwrap();
    assert_eq!(round_trip, refusal);
    assert!(service.head(MISSION).is_none());

    let malformed = serde_json::to_vec(&json!({
        "action": "execution_dispatch",
        "schema": MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA,
        "request_id": "request-malformed",
        "unknown": true
    }))
    .unwrap();
    let error = dispatch_external_mission_json(&mut service, None, &malformed, NOW).unwrap_err();
    assert_eq!(error.code(), "invalid_transport_request");
}
