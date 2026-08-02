use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::{to_bytes, Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode};
use m1nd_control::{ExecutionDispatchState, MissionState, Role};
use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio::sync::broadcast;
use tower::ServiceExt;

use crate::brain_runtime::BrainSessionCell;
use crate::execution_dispatch::{
    ExecutionMissionHeadV1, RunnerClaimOutcome, RunnerExecutionInbox, RunnerInboxState,
    EXECUTION_MISSION_HEAD_SCHEMA,
};
use crate::http_server::{build_router, AppState, SseEvent};
use crate::mission_service::{AuthenticatedAuthorityContextV1, MissionService};
use crate::mission_service_tests::{
    authority, build_land, config, dispatch, execution_ack, hash, payload, service_decision,
    successful_result_and_candidate, transition_intent, BRAIN, MISSION, NOW, RUNNER, SERVICE_ACTOR,
};
use crate::mission_service_transport::{
    ExternalMissionServiceRequestV1, MissionServiceAuthorityProvider, MissionServiceTransportError,
    MissionServiceTransportFacade, MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA,
};
use crate::server::{tool_schemas, McpConfig};

type AuthorityOverrides = Arc<Mutex<BTreeMap<String, AuthenticatedAuthorityContextV1>>>;

fn transport_schema() -> String {
    MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA.to_string()
}

fn verified_facade(
    root: &std::path::Path,
    overrides: AuthorityOverrides,
) -> Arc<MissionServiceTransportFacade> {
    let provider: Arc<dyn MissionServiceAuthorityProvider> =
        Arc::new(
            move |_context: &crate::mission_service_transport::MissionServiceTransportContextV1,
                  request: &ExternalMissionServiceRequestV1,
                  object_digest: &str,
                  _owner_now_ms: u64|
                  -> Result<
                Option<AuthenticatedAuthorityContextV1>,
                MissionServiceTransportError,
            > {
                if let Some(authority) = overrides.lock().get(object_digest).cloned() {
                    return Ok(Some(authority));
                }
                let intent = match request {
                    ExternalMissionServiceRequestV1::MissionTransition { intent, .. }
                    | ExternalMissionServiceRequestV1::ExecutionDispatch { intent, .. }
                    | ExternalMissionServiceRequestV1::ExecutionStarted { intent, .. }
                    | ExternalMissionServiceRequestV1::ExecutionTerminal { intent, .. } => intent,
                    ExternalMissionServiceRequestV1::LandIntent { .. } => {
                        return Ok(Some(authority(
                            "wire-reader",
                            Role::Author,
                            "cap-wire-land-intent",
                            object_digest,
                        )))
                    }
                    ExternalMissionServiceRequestV1::Land { .. } => return Ok(None),
                };
                Ok(Some(authority(
                    &intent.actor_id,
                    intent.role,
                    &intent.capability_id,
                    object_digest,
                )))
            },
        );
    Arc::new(
        MissionServiceTransportFacade::open_with_clock_software_test_not_production(
            root,
            config(),
            provider,
            Arc::new(|| NOW),
        )
        .expect("open verified wire facade"),
    )
}

fn missing_authority_facade(root: &std::path::Path) -> Arc<MissionServiceTransportFacade> {
    let provider: Arc<dyn MissionServiceAuthorityProvider> = Arc::new(
        |_context: &crate::mission_service_transport::MissionServiceTransportContextV1,
         _request: &ExternalMissionServiceRequestV1,
         _object_digest: &str,
         _owner_now_ms: u64|
         -> Result<Option<AuthenticatedAuthorityContextV1>, MissionServiceTransportError> {
            Ok(None)
        },
    );
    Arc::new(
        MissionServiceTransportFacade::open_with_clock_software_test_not_production(
            root,
            config(),
            provider,
            Arc::new(|| NOW),
        )
        .expect("open unauthorized wire facade"),
    )
}

fn wire_app(
    runtime: &std::path::Path,
    mission_service: Option<Arc<MissionServiceTransportFacade>>,
) -> Arc<AppState> {
    std::fs::create_dir_all(runtime).expect("create wire runtime");
    let config = McpConfig {
        graph_source: runtime.join("graph.json"),
        plasticity_state: runtime.join("plasticity.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        registry_dir: Some(runtime.join("registry")),
        ..Default::default()
    };
    let session = crate::server::McpServer::new(config)
        .expect("boot wire owner")
        .into_session_state();
    let (event_tx, _) = broadcast::channel::<SseEvent>(16);
    let tool_schemas_cache = tool_schemas()
        .get("tools")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    Arc::new(AppState {
        session: Arc::new(BrainSessionCell::new(session)),
        tool_schemas_cache,
        event_tx,
        event_log_path: None,
        registry_dir: Some(runtime.join("registry")),
        mcp_sessions: crate::mcp_http::new_mcp_session_registry(),
        project_brains: Arc::new(crate::project_brains::ProjectBrainRegistry::new(
            runtime.join(crate::project_brains::PROJECT_BRAINS_DIR),
            None,
        )),
        runnerd: Arc::new(crate::runnerd_owner::RunnerdRegistry::default()),
        ui_authority: Arc::new(crate::ui_attestation::UiBundleAttestor::default()),
        mission_service,
        external_mutation_service: None,
        authority_service: None,
        autonomy_owner: None,
    })
}

async fn rest_raw(router: &axum::Router, tool: &str, body: Vec<u8>) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/tools/{tool}"))
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("REST wire response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("REST response bytes");
    let value = serde_json::from_slice(&bytes).expect("REST response JSON");
    (status, value)
}

async fn rest_call(router: &axum::Router, tool: &str, body: &Value) -> (StatusCode, Value) {
    rest_raw(router, tool, serde_json::to_vec(body).unwrap()).await
}

fn bound_caller_root(app: &Arc<AppState>) -> String {
    app.project_brains
        .bound_actor_root_for_target(Arc::clone(&app.session))
        .expect("bound owner exposes its exact actor root")
}

async fn mcp_session(app: &Arc<AppState>) -> String {
    let initialize = Bytes::from(
        serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        }))
        .unwrap(),
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        "m1nd-caller-root",
        bound_caller_root(app)
            .parse()
            .expect("valid caller-root header"),
    );
    let response = crate::mcp_http::handle_mcp_post(State(app.clone()), headers, initialize).await;
    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .expect("initialize returns MCP session")
        .to_string()
}

async fn mcp_call(
    app: &Arc<AppState>,
    session_id: &str,
    tool: &str,
    arguments: &Value,
) -> (bool, Value) {
    let mut headers = HeaderMap::new();
    headers.insert("mcp-session-id", session_id.parse().unwrap());
    headers.insert(
        "m1nd-caller-root",
        bound_caller_root(app)
            .parse()
            .expect("valid caller-root header"),
    );
    let body = Bytes::from(
        serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": tool, "arguments": arguments }
        }))
        .unwrap(),
    );
    let response = crate::mcp_http::handle_mcp_post(State(app.clone()), headers, body).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("MCP response bytes");
    let envelope: Value = serde_json::from_slice(&bytes).expect("MCP JSON-RPC response");
    let is_error = envelope["result"]["isError"].as_bool().unwrap_or(false);
    let text = envelope["result"]["content"][0]["text"]
        .as_str()
        .expect("MCP text content");
    let payload = serde_json::from_str(text).expect("MCP content carries JSON payload");
    (is_error, payload)
}

fn open_snapshot(root: &std::path::Path) -> MissionService {
    MissionService::open_software_test_not_production(root, config())
        .expect("open MissionService snapshot")
}

#[tokio::test]
async fn real_wires_desurface_and_refuse_legacy_before_invalid_body_parsing() {
    let directory = tempfile::tempdir().unwrap();
    let app = wire_app(&directory.path().join("owner"), None);
    let router = build_router(app.clone(), false);

    let list_response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tools")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list: Value = serde_json::from_slice(
        &to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let names: Vec<&str> = list["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    // SEMANTICS FIXED 2026-08-01 with the core-menu change, deliberately.
    //
    // This guard has two halves and they judge two different sets. The POSITIVE
    // half says the typed replacements still EXIST — desurfacing the legacy raw
    // writes must not have taken `mission_service` or the authority flow with
    // them. Existence is a property of the REGISTRY, which the core menu does
    // not touch; these four verbs are simply no longer advertised, and reading
    // the advertised list for them would have turned a real guard into a test of
    // the shop window's contents.
    //
    // The NEGATIVE half is untouched in meaning and is now checked against BOTH
    // sets, which is strictly stronger than before: `mission_post` and
    // `receipt_import` were REMOVED, so they must be absent from the registry
    // itself, not merely hidden from the menu.
    let registry = crate::server::all_tool_schemas();
    let registered: Vec<&str> = registry["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    for replacement in [
        "mission_service",
        "authority_session_challenge",
        "authority_session_authenticate",
        "authority_authorize",
    ] {
        assert!(
            registered.contains(&replacement),
            "{replacement} must remain registered and callable"
        );
    }
    for desurfaced in ["mission_post", "receipt_import"] {
        assert!(
            !registered.contains(&desurfaced),
            "{desurfaced} was removed from the surface, not merely unadvertised"
        );
        assert!(!names.contains(&desurfaced));
    }

    for legacy in ["mission_post", "receipt_import", "landed"] {
        let (status, refusal) = rest_raw(&router, legacy, b"{not-json".to_vec()).await;
        assert_eq!(status, StatusCode::GONE, "{legacy}");
        assert_eq!(refusal["code"], "legacy_direct_mutation_refused");
    }

    let session = mcp_session(&app).await;
    for legacy in ["mission_post", "receipt_import", "landed"] {
        let (is_error, refusal) = mcp_call(
            &app,
            &session,
            legacy,
            &json!({"authority": {"forged": true}, "payload": "untyped"}),
        )
        .await;
        assert!(is_error, "{legacy}");
        assert_eq!(refusal["code"], "legacy_direct_mutation_refused");
    }
}

#[tokio::test]
async fn authority_issuance_is_a_distinct_fail_closed_rest_and_mcp_surface() {
    let directory = tempfile::tempdir().unwrap();
    let app = wire_app(&directory.path().join("owner"), None);
    let router = build_router(app.clone(), false);

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/authority/authorize?brain=brain-1")
                .header("content-type", "application/json")
                .header("m1nd-transport-session-id", "rest-session-1")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let refusal: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(refusal["code"], "authority_service_unavailable");

    for path in [
        "/api/authority/session/challenge?brain=brain-1",
        "/api/authority/session/authenticate?brain=brain-1",
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("content-type", "application/json")
                    .header("m1nd-transport-session-id", "rest-session-1")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let refusal: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(refusal["code"], "authority_service_unavailable");
    }

    let session = mcp_session(&app).await;
    for tool in [
        "authority_session_challenge",
        "authority_session_authenticate",
        "authority_authorize",
    ] {
        let (is_error, refusal) = mcp_call(&app, &session, tool, &json!({})).await;
        assert!(is_error, "{tool}");
        assert_eq!(refusal["code"], "authority_service_unavailable", "{tool}");
    }
}

#[tokio::test]
async fn real_wires_fail_closed_without_sovereign_authority() {
    let directory = tempfile::tempdir().unwrap();
    let service_root = directory.path().join("mission");
    let facade = missing_authority_facade(&service_root);
    let app = wire_app(&directory.path().join("owner"), Some(facade));
    let router = build_router(app.clone(), false);
    let packet = hash('3');
    let service = open_snapshot(&service_root);
    let execution = dispatch("wire-unauthorized", &packet);
    let transition_payload = payload(
        crate::mission_service::MissionTransitionEvidenceV1::MissionServiceDecision {
            decision: service_decision("wire-unauthorized-decision"),
            dispatch: Some(execution),
        },
    );
    let intent = transition_intent(
        &service,
        "wire-unauthorized",
        MissionState::Dispatching,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-wire-unauthorized",
        &transition_payload,
        packet,
        "idem-wire-unauthorized",
    );
    drop(service);
    let request = serde_json::to_value(ExternalMissionServiceRequestV1::ExecutionDispatch {
        schema: transport_schema(),
        request_id: "request-wire-unauthorized".to_string(),
        intent,
        payload: transition_payload,
    })
    .unwrap();

    let (status, refusal) = rest_call(&router, "mission_service", &request).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(refusal["code"], "missing_authenticated_authority");
    let session = mcp_session(&app).await;
    let (is_error, refusal) = mcp_call(&app, &session, "mission_service", &request).await;
    assert!(is_error);
    assert_eq!(refusal["code"], "missing_authenticated_authority");
    assert!(open_snapshot(&service_root).head(MISSION).is_none());
}

#[tokio::test]
async fn typed_rest_and_mcp_lifecycle_replays_and_survives_facade_restart() {
    let directory = tempfile::tempdir().unwrap();
    let service_root = directory.path().join("mission");
    let overrides = AuthorityOverrides::default();
    let facade = verified_facade(&service_root, overrides.clone());
    let app = wire_app(&directory.path().join("owner"), Some(facade));
    let router = build_router(app.clone(), false);
    let session = mcp_session(&app).await;
    let packet = hash('3');

    let execution = dispatch("wire-dispatch", &packet);
    let service = open_snapshot(&service_root);
    let dispatch_payload = payload(
        crate::mission_service::MissionTransitionEvidenceV1::MissionServiceDecision {
            decision: service_decision("wire-dispatch-decision"),
            dispatch: Some(execution.clone()),
        },
    );
    let dispatch_intent = transition_intent(
        &service,
        "wire-dispatch",
        MissionState::Dispatching,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-wire-dispatch",
        &dispatch_payload,
        packet.clone(),
        "idem-wire-dispatch",
    );
    drop(service);
    let dispatch_request =
        serde_json::to_value(ExternalMissionServiceRequestV1::ExecutionDispatch {
            schema: transport_schema(),
            request_id: "request-wire-dispatch".to_string(),
            intent: dispatch_intent,
            payload: dispatch_payload,
        })
        .unwrap();
    let (status, first_dispatch) = rest_call(&router, "mission_service", &dispatch_request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        first_dispatch["result"]["outcome"]["letter"]["state"],
        "dispatching"
    );
    let (is_error, replay_dispatch) =
        mcp_call(&app, &session, "mission_service", &dispatch_request).await;
    assert!(!is_error);
    assert_eq!(replay_dispatch["result"]["outcome"]["deduplicated"], true);

    let runner_path = directory.path().join("runner-inbox.jsonl");
    let started_snapshot = {
        let mut runner = RunnerExecutionInbox::open(&runner_path, RUNNER).unwrap();
        let claim = match runner.claim_for_spawn(execution.clone(), NOW - 45).unwrap() {
            RunnerClaimOutcome::Spawn(permit) => permit.claim,
            RunnerClaimOutcome::AlreadyClaimed { .. } => panic!("first claim must spawn"),
        };
        runner
            .mark_process_started(
                &execution.execution_id,
                &claim.claim_id,
                "pid:wire:start:1",
                NOW - 40,
            )
            .unwrap();
        runner.get(&execution.execution_id).unwrap().clone()
    };
    assert_eq!(started_snapshot.state, RunnerInboxState::Started);
    let service = open_snapshot(&service_root);
    let ack = execution_ack(&execution);
    let ack_payload = payload(
        crate::mission_service::MissionTransitionEvidenceV1::ExecutionDispatchAck {
            ack: ack.clone(),
        },
    );
    let ack_intent = transition_intent(
        &service,
        "wire-started",
        MissionState::Executing,
        RUNNER,
        Role::Runner,
        "cap-wire-started",
        &ack_payload,
        packet.clone(),
        "idem-wire-started",
    );
    drop(service);
    let started_request = serde_json::to_value(ExternalMissionServiceRequestV1::ExecutionStarted {
        schema: transport_schema(),
        request_id: "request-wire-started".to_string(),
        snapshot: started_snapshot,
        intent: ack_intent,
        payload: ack_payload,
    })
    .unwrap();
    let (is_error, executing) = mcp_call(&app, &session, "mission_service", &started_request).await;
    assert!(!is_error);
    assert_eq!(
        executing["result"]["outcome"]["letter"]["state"],
        "executing"
    );
    let (is_error, executing_replay) =
        mcp_call(&app, &session, "mission_service", &started_request).await;
    assert!(!is_error);
    assert_eq!(executing_replay["result"]["outcome"]["deduplicated"], true);

    let executing_head = open_snapshot(&service_root).head(MISSION).unwrap().clone();
    let terminal_snapshot = {
        let mut runner = RunnerExecutionInbox::open(&runner_path, RUNNER).unwrap();
        runner.record_ack(ack.clone(), NOW).unwrap();
        runner
            .observe_executing_transition(
                &execution.execution_id,
                &ack.ack_digest,
                ExecutionMissionHeadV1 {
                    schema: EXECUTION_MISSION_HEAD_SCHEMA.to_string(),
                    head_id: executing_head.head_id.clone(),
                    state: MissionState::Executing,
                    iteration_id: executing_head.iteration_id,
                    packet_digest: executing_head.packet_digest.clone(),
                },
                NOW,
            )
            .unwrap();
        let service = open_snapshot(&service_root);
        let (result, _candidate) = successful_result_and_candidate(&service, &execution);
        drop(service);
        runner.record_result(result, NOW).unwrap();
        runner.get(&execution.execution_id).unwrap().clone()
    };
    assert_eq!(terminal_snapshot.state, RunnerInboxState::Completed);
    let service = open_snapshot(&service_root);
    let (result, candidate) = successful_result_and_candidate(&service, &execution);
    let terminal_payload = payload(
        crate::mission_service::MissionTransitionEvidenceV1::ExecutionResult {
            result: result.clone(),
            candidate: Some(candidate.clone()),
        },
    );
    let terminal_intent = transition_intent(
        &service,
        "wire-terminal",
        MissionState::Gate,
        RUNNER,
        Role::Runner,
        "cap-wire-terminal",
        &terminal_payload,
        packet.clone(),
        "idem-wire-terminal",
    );
    drop(service);
    let terminal_request =
        serde_json::to_value(ExternalMissionServiceRequestV1::ExecutionTerminal {
            schema: transport_schema(),
            request_id: "request-wire-terminal".to_string(),
            snapshot: terminal_snapshot,
            intent: terminal_intent,
            payload: terminal_payload,
        })
        .unwrap();
    let (status, gate) = rest_call(&router, "mission_service", &terminal_request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(gate["result"]["outcome"]["letter"]["state"], "gate");

    let service = open_snapshot(&service_root);
    let merge_payload = payload(
        crate::mission_service::MissionTransitionEvidenceV1::MissionServiceDecision {
            decision: service_decision("wire-merge-wait-decision"),
            dispatch: None,
        },
    );
    let merge_intent = transition_intent(
        &service,
        "wire-merge-wait",
        MissionState::MergeWait,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-wire-merge-wait",
        &merge_payload,
        packet,
        "idem-wire-merge-wait",
    );
    drop(service);
    let merge_request = serde_json::to_value(ExternalMissionServiceRequestV1::MissionTransition {
        schema: transport_schema(),
        request_id: "request-wire-merge-wait".to_string(),
        intent: merge_intent,
        payload: merge_payload,
    })
    .unwrap();
    let (is_error, merge_wait) = mcp_call(&app, &session, "mission_service", &merge_request).await;
    assert!(!is_error);
    assert_eq!(
        merge_wait["result"]["outcome"]["letter"]["state"],
        "merge_wait"
    );

    let merge_head = open_snapshot(&service_root).head(MISSION).unwrap().clone();
    let merge_candidate = merge_head.receipt_candidate.as_ref().unwrap();
    let land_intent_request = serde_json::to_value(ExternalMissionServiceRequestV1::LandIntent {
        schema: transport_schema(),
        request_id: "request-wire-land-intent".to_string(),
        mission_id: MISSION.to_string(),
        expected_head_id: merge_head.head_id.clone(),
        candidate_id: merge_candidate.candidate_id.clone(),
        expected_candidate_digest: merge_candidate.candidate_digest.clone(),
        expected_store_version: merge_head.store_version,
        idempotency_key: "idem-wire-land".to_string(),
    })
    .unwrap();
    let (status, land_intent) = rest_call(&router, "mission_service", &land_intent_request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        land_intent["result"]["intent"]["expected_head_id"],
        merge_head.head_id
    );

    let service = open_snapshot(&service_root);
    let (land_authority, land_request) =
        build_land(&service, "wire-land-transaction", "idem-wire-land");
    let transaction_digest = land_request.transaction.transaction_digest().to_string();
    drop(service);
    overrides.lock().insert(transaction_digest, land_authority);
    let land_request = serde_json::to_value(ExternalMissionServiceRequestV1::Land {
        schema: transport_schema(),
        request_id: "request-wire-land".to_string(),
        request: land_request,
    })
    .unwrap();
    let (is_error, landed) = mcp_call(&app, &session, "mission_service", &land_request).await;
    assert!(!is_error);
    assert_eq!(landed["result"]["outcome"]["deduplicated"], false);

    // Recreate both facade and owner AppState against the same durable root.
    let restarted = verified_facade(&service_root, overrides);
    let restarted_app = wire_app(&directory.path().join("owner-restarted"), Some(restarted));
    let restarted_router = build_router(restarted_app, false);
    let (status, replay) = rest_call(&restarted_router, "mission_service", &land_request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["result"]["outcome"]["deduplicated"], true);
    assert_eq!(
        replay["result"]["outcome"]["receipt_id"],
        landed["result"]["outcome"]["receipt_id"]
    );

    let final_service = open_snapshot(&service_root);
    assert_eq!(
        final_service.head(MISSION).unwrap().state,
        MissionState::Landed
    );
    assert_eq!(final_service.receipts().len(), 1);
    assert_eq!(
        final_service
            .execution_dispatch(&execution.execution_id)
            .unwrap()
            .state,
        ExecutionDispatchState::Completed
    );
}
