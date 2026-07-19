use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::{to_bytes, Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode};
use m1nd_control::{Effect, Ingress, MissionState, Role};
use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio::sync::broadcast;
use tower::ServiceExt;

use crate::brain_runtime::BrainSessionCell;
use crate::http_server::{build_router, AppState, SseEvent};
use crate::mission_service::AuthenticatedAuthorityContextV1;
use crate::mission_service_tests::{
    authority, config, dispatch, hash, payload, service_decision, transition_intent, NOW,
    SERVICE_ACTOR,
};
use crate::mission_service_transport::{
    ExternalMissionServiceRequestV1, MissionServiceAuthorityProvider, MissionServiceTransportError,
    MissionServiceTransportFacade, MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA,
};
use crate::server::{dispatch_tool, tool_schemas, McpConfig};

type AuthorityOverrides = Arc<Mutex<BTreeMap<String, AuthenticatedAuthorityContextV1>>>;

fn projected_facade(
    service_root: &std::path::Path,
    evidence_root: &std::path::Path,
    workspace: &std::path::Path,
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
                    ExternalMissionServiceRequestV1::LandIntent { .. }
                    | ExternalMissionServiceRequestV1::Land { .. } => return Ok(None),
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
        MissionServiceTransportFacade::open_with_clock_and_evidence_spine(
            service_root,
            config(),
            provider,
            Arc::new(|| NOW),
            evidence_root,
            workspace,
        )
        .expect("open G3 facade with G5 projection"),
    )
}

fn wire_app(
    runtime: &std::path::Path,
    workspace: &std::path::Path,
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
    let mut session = crate::server::McpServer::new(config)
        .expect("boot G5 wire owner")
        .into_session_state();
    session.workspace_root = Some(
        std::fs::canonicalize(workspace)
            .expect("canonical workspace")
            .to_string_lossy()
            .to_string(),
    );
    session.workspace_root_source = Some("g5_wire_owner_selection".to_string());
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
    let response =
        crate::mcp_http::handle_mcp_post(State(app.clone()), HeaderMap::new(), initialize).await;
    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .expect("initialize returns MCP session")
        .to_string()
}

async fn mcp_call_raw(
    app: &Arc<AppState>,
    session_id: &str,
    tool: &str,
    arguments: &Value,
) -> (bool, String) {
    let mut headers = HeaderMap::new();
    headers.insert("mcp-session-id", session_id.parse().unwrap());
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
        .expect("MCP text content")
        .to_string();
    (is_error, text)
}

async fn mcp_call(
    app: &Arc<AppState>,
    session_id: &str,
    tool: &str,
    arguments: &Value,
) -> (bool, Value) {
    let (is_error, text) = mcp_call_raw(app, session_id, tool, arguments).await;
    let payload = serde_json::from_str(&text).expect("MCP content carries JSON payload");
    (is_error, payload)
}

fn initial_dispatch_request(service_root: &std::path::Path) -> Value {
    let service = crate::mission_service::MissionService::open_software_test_not_production(
        service_root,
        config(),
    )
    .expect("open G3 snapshot");
    let packet = hash('3');
    let execution = dispatch("g5-wire-dispatch", &packet);
    let transition_payload = payload(
        crate::mission_service::MissionTransitionEvidenceV1::MissionServiceDecision {
            decision: service_decision("g5-wire-dispatch-decision"),
            dispatch: Some(execution),
        },
    );
    let intent = transition_intent(
        &service,
        "g5-wire-dispatch",
        MissionState::Dispatching,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-g5-wire-dispatch",
        &transition_payload,
        packet,
        "idem-g5-wire-dispatch",
    );
    serde_json::to_value(ExternalMissionServiceRequestV1::ExecutionDispatch {
        schema: MISSION_SERVICE_TRANSPORT_REQUEST_SCHEMA.to_string(),
        request_id: "request-g5-wire-dispatch".to_string(),
        intent,
        payload: transition_payload,
    })
    .unwrap()
}

fn evidence_files(root: &std::path::Path) -> (Vec<u8>, Vec<u8>, Vec<String>) {
    let identity = std::fs::read(root.join("identity.json")).expect("evidence identity");
    let log = std::fs::read(root.join("correlations.jsonl")).expect("evidence log");
    let mut locks = std::fs::read_dir(root.join(".locks"))
        .expect("evidence locks")
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    locks.sort();
    (identity, log, locks)
}

fn durable_tree_snapshot(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    fn visit(base: &std::path::Path, path: &std::path::Path, rows: &mut Vec<(String, Vec<u8>)>) {
        if !path.exists() {
            return;
        }
        let mut entries = std::fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        entries.sort();
        for entry in entries {
            if entry.is_dir() {
                visit(base, &entry, rows);
            } else {
                let mut bytes = std::fs::read(&entry).unwrap();
                // The actor heartbeat is intentionally independent of request
                // traffic and may advance while this wire test runs. Normalize
                // that sole liveness field so the snapshot still detects any
                // forged presence/lease mutation without racing the owner clock.
                if entry.extension().and_then(|extension| extension.to_str()) == Some("json") {
                    if let Ok(mut value) = serde_json::from_slice::<Value>(&bytes) {
                        if let Some(object) = value.as_object_mut() {
                            if object.contains_key("last_heartbeat_ms") {
                                object.insert("last_heartbeat_ms".into(), Value::from(0));
                            }
                        }
                        bytes = serde_json::to_vec(&value).unwrap();
                    }
                }
                rows.push((
                    entry
                        .strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                    bytes,
                ));
            }
        }
    }
    let mut rows = Vec::new();
    visit(root, root, &mut rows);
    rows
}

#[tokio::test]
async fn canonical_g3_projection_is_queryable_read_only_over_rest_and_streamable_mcp_after_restart()
{
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    let runtime = directory.path().join("owner");
    let service_root = directory.path().join("mission-service");
    let evidence_root = runtime.join("evidence-spine");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&runtime).unwrap();
    let overrides = AuthorityOverrides::default();
    let request = initial_dispatch_request(&service_root);

    let first_core = {
        let facade = projected_facade(&service_root, &evidence_root, &workspace, overrides.clone());
        let app = wire_app(&runtime, &workspace, Some(facade));
        let router = build_router(app.clone(), false);
        let session = mcp_session(&app).await;

        let (status, transition) = rest_call(&router, "mission_service", &request).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(transition["evidence_projection"]["status"], "synchronized");
        let link = transition["evidence_projection"]["evidence_link"].clone();
        assert_eq!(
            link["mission_id"],
            transition["result"]["outcome"]["letter"]["mission_id"]
        );
        assert_eq!(
            link["mission_head_id"],
            transition["result"]["outcome"]["letter"]["head_id"]
        );

        let before_queries = evidence_files(&evidence_root);
        let (status, rest) = rest_call(
            &router,
            "evidence_query",
            &json!({"mission_id": link["mission_id"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let rest_core = rest["result"].clone();
        assert_eq!(rest_core["verified_rows"], 1);
        assert_eq!(rest_core["correlations"].as_array().unwrap().len(), 1);
        assert_eq!(rest_core["integrity"], "hash_chain_verified_committed_rows");

        let (is_error, mcp) = mcp_call(
            &app,
            &session,
            "evidence_query",
            &json!({"mission_head_id": link["mission_head_id"]}),
        )
        .await;
        assert!(!is_error);
        assert_eq!(mcp["chain_head_digest"], rest_core["chain_head_digest"]);
        assert_eq!(mcp["correlations"], rest_core["correlations"]);
        assert_eq!(evidence_files(&evidence_root), before_queries);

        let (status, invalid) = rest_call(
            &router,
            "evidence_query",
            &json!({"brain_id": "brain-forged"}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(invalid["error"], "invalid_params");
        assert!(invalid["message"]
            .as_str()
            .unwrap()
            .contains("unknown field `brain_id`"));
        let (is_error, invalid) = mcp_call_raw(
            &app,
            &session,
            "evidence_query",
            &json!({"brain_id": "brain-forged"}),
        )
        .await;
        assert!(is_error);
        assert!(invalid.contains("unknown field `brain_id`"));

        // Even a refused payload carrying agent_id must remain read-only. The
        // generic REST/MCP wrappers normally track agents before dispatch, so
        // this locks the pre-decode exception for every accepted alias.
        let registry_root = runtime.join("registry");
        let registry_before = durable_tree_snapshot(&registry_root);
        let evidence_before = evidence_files(&evidence_root);
        let sessions_before = app
            .project_brains
            .read_target_runtime_snapshot(Arc::clone(&app.session), None, true, |state| {
                Ok::<_, crate::runtime_jobs::RuntimeJobFailure>(state.sessions.len())
            })
            .expect("read session count through bound actor")
            .value;
        for alias in [
            "evidence_query",
            "m1nd.evidence_query",
            "m1nd_evidence_query",
        ] {
            let (status, _invalid) =
                rest_call(&router, alias, &json!({"agent_id": "forged-presence"})).await;
            assert!(status.is_client_error(), "REST alias {alias}");
            let (is_error, _invalid) = mcp_call_raw(
                &app,
                &session,
                alias,
                &json!({"agent_id": "forged-presence"}),
            )
            .await;
            assert!(is_error, "MCP alias {alias}");
        }
        let session_observation = app
            .project_brains
            .read_target_runtime_snapshot(Arc::clone(&app.session), None, true, |state| {
                Ok::<_, crate::runtime_jobs::RuntimeJobFailure>((
                    state.sessions.len(),
                    state.sessions.contains_key("forged-presence"),
                ))
            })
            .expect("read session observation through bound actor")
            .value;
        assert_eq!(session_observation.0, sessions_before);
        assert!(!session_observation.1);
        assert_eq!(durable_tree_snapshot(&registry_root), registry_before);
        assert_eq!(evidence_files(&evidence_root), evidence_before);

        for legacy in ["mission_post", "receipt_import", "landed"] {
            let (status, refusal) = rest_raw(&router, legacy, b"{not-json".to_vec()).await;
            assert_eq!(status, StatusCode::GONE, "{legacy}");
            assert_eq!(refusal["code"], "legacy_direct_mutation_refused");
            let (is_error, refusal) = mcp_call(
                &app,
                &session,
                legacy,
                &json!({"authority": {"forged": true}}),
            )
            .await;
            assert!(is_error, "{legacy}");
            assert_eq!(refusal["code"], "legacy_direct_mutation_refused");
        }
        rest_core
    };

    let before_restart = evidence_files(&evidence_root);
    let restarted_facade = projected_facade(&service_root, &evidence_root, &workspace, overrides);
    assert_eq!(evidence_files(&evidence_root), before_restart);
    let restarted_app = wire_app(&runtime, &workspace, Some(restarted_facade.clone()));
    let restarted_router = build_router(restarted_app.clone(), false);
    let (status, restarted) = rest_call(&restarted_router, "evidence_query", &json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        restarted["result"]["chain_head_digest"],
        first_core["chain_head_digest"]
    );
    assert_eq!(
        restarted["result"]["correlations"],
        first_core["correlations"]
    );
    assert_eq!(evidence_files(&evidence_root), before_restart);

    let other_workspace = directory.path().join("other-workspace");
    std::fs::create_dir_all(&other_workspace).unwrap();
    // The restarted owner is already actor-fenced after `build_router`; do not
    // mutate its SessionState through the compatibility cell.  A second owner
    // with its binding fixed before actor startup exercises the same wire
    // refusal without creating a post-startup side channel around the actor.
    let wrong_runtime = directory.path().join("wrong-runtime");
    // `evidence_query` resolves its read-only spine from the selected owner's
    // runtime root, not from the MissionService facade. Seed the persisted
    // spine bytes into this independent owner so the refusal exercises the
    // intended identity mismatch instead of the unrelated
    // `evidence_spine_not_configured` branch.
    let wrong_evidence_root = wrong_runtime.join("evidence-spine");
    std::fs::create_dir_all(&wrong_evidence_root).unwrap();
    std::fs::copy(
        evidence_root.join("identity.json"),
        wrong_evidence_root.join("identity.json"),
    )
    .unwrap();
    std::fs::copy(
        evidence_root.join("correlations.jsonl"),
        wrong_evidence_root.join("correlations.jsonl"),
    )
    .unwrap();
    let wrong_app = wire_app(
        &wrong_runtime,
        &other_workspace,
        Some(restarted_facade.clone()),
    );
    let wrong_router = build_router(wrong_app, false);
    let (status, wrong_workspace) = rest_call(&wrong_router, "evidence_query", &json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(wrong_workspace["error"], "invalid_params");
    assert!(wrong_workspace["message"]
        .as_str()
        .unwrap()
        .contains("wrong_workspace_binding"));
    assert_eq!(evidence_files(&evidence_root), before_restart);
}

#[test]
fn evidence_route_catalog_is_read_only_and_stdio_cannot_bypass_the_g3_wire() {
    let catalog = m1nd_control::m1nd10_action_catalog().unwrap();
    let entry = catalog
        .entries
        .iter()
        .find(|entry| entry.action.as_str() == "evidence.query")
        .expect("evidence.query catalog entry");
    assert_eq!(
        entry.complete_effects.iter().copied().collect::<Vec<_>>(),
        [Effect::Read]
    );
    assert!(entry.ingresses.contains(&Ingress::Rest));
    assert!(entry.ingresses.contains(&Ingress::Mcp));

    let directory = tempfile::tempdir().unwrap();
    let config = McpConfig {
        graph_source: directory.path().join("graph.json"),
        plasticity_state: directory.path().join("plasticity.json"),
        runtime_dir: Some(directory.path().to_path_buf()),
        registry_dir: Some(directory.path().join("registry")),
        ..Default::default()
    };
    let mut state = crate::server::McpServer::new(config)
        .unwrap()
        .into_session_state();
    let error = dispatch_tool(&mut state, "mission_service", &json!({})).unwrap_err();
    assert!(error.to_string().contains("mission_service_unavailable"));
    for legacy in ["mission_post", "receipt_import", "landed"] {
        let error = dispatch_tool(&mut state, legacy, &json!({"forged": true})).unwrap_err();
        assert!(error.to_string().contains("legacy_direct_mutation_refused"));
    }
}
