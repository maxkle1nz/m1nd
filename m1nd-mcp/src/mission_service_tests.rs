use std::collections::BTreeSet;
use std::sync::{mpsc, Arc, Barrier};

use m1nd_control::{
    ActiveMode, AuthorityTransactionBindingV1, AuthorityTransactionV1, AuthorityVariant,
    CapabilityKind, Effect, ExecutionDispatchAckV1, ExecutionDispatchState, ExecutionDispatchV1,
    ExecutionOutcome, ExecutionResultV1, Ingress, MissionState, MissionTransitionIntentV1,
    MissionTransitionSource, OpaqueSignature, PositiveAuthorityTransactionV1, Role,
    CANONICALIZATION_VERSION, EXECUTION_DISPATCH_ACK_SCHEMA, EXECUTION_DISPATCH_SCHEMA,
    EXECUTION_RESULT_SCHEMA, MISSION_TRANSITION_INTENT_SCHEMA,
    POSITIVE_AUTHORITY_TRANSACTION_SCHEMA,
};
use tempfile::TempDir;

use super::mission_service::*;
use crate::execution_dispatch::{
    ExecutionMissionHeadV1, OwnerReconciliationAction, RunnerClaimOutcome, RunnerExecutionInbox,
    RunnerInboxState, EXECUTION_MISSION_HEAD_SCHEMA,
};
use crate::system_blocks::ReceiptType;

pub(crate) const NOW: u64 = 1_000_000;
pub(crate) const BRAIN: &str = "brain-1";
const ORGANISM: &str = "organism-1";
pub(crate) const MISSION: &str = "mission-1";
const BLOCK: &str = "block-1";
pub(crate) const SERVICE_ACTOR: &str = "mission-service-1";
pub(crate) const RUNNER: &str = "runner-1";
const HUMAN: &str = "human-1";
const EVIDENCE_LOCATOR: &str = "proofs/g3-run.log";

pub(crate) fn hash(byte: char) -> String {
    byte.to_string().repeat(64)
}

pub(crate) fn config() -> MissionServiceConfigV1 {
    MissionServiceConfigV1 {
        schema: MISSION_SERVICE_CONFIG_SCHEMA.to_string(),
        organism_id: ORGANISM.to_string(),
        brain_id: BRAIN.to_string(),
        mission_service_actor_id: SERVICE_ACTOR.to_string(),
        canonical_blocks: vec![CanonicalBlockBindingV1 {
            block_id: BLOCK.to_string(),
            store_version: 7,
            boundary_version: 3,
            contract_version: 2,
            resolution_hash: hash('a'),
        }],
        canonical_evidence: vec![CanonicalEvidenceAnchorV1 {
            locator: EVIDENCE_LOCATOR.to_string(),
            sha256: hash('b'),
            producer_id: RUNNER.to_string(),
        }],
    }
}

pub(crate) fn open_service(directory: &TempDir) -> MissionService {
    MissionService::open_software_test_not_production(directory.path(), config()).unwrap()
}

pub(crate) fn authority(
    subject: &str,
    role: Role,
    capability_id: &str,
    object_digest: &str,
) -> AuthenticatedAuthorityContextV1 {
    AuthenticatedAuthorityContextV1 {
        schema: AUTHENTICATED_AUTHORITY_CONTEXT_SCHEMA.to_string(),
        organism_id: ORGANISM.to_string(),
        brain_id: BRAIN.to_string(),
        subject_id: subject.to_string(),
        role,
        capability_id: capability_id.to_string(),
        capability_kind: Some(CapabilityKind::Human),
        authority_variant: AuthorityVariant::Human,
        active_mode: ActiveMode::HumanGated,
        mission_id: Some(MISSION.to_string()),
        mission_head_id: None,
        transport_session_id: "test-transport-session".to_string(),
        ingress_context_digest: hash('9'),
        action_id: "mission.service.mission_transition".to_string(),
        ingress: Ingress::Rest,
        complete_effects: BTreeSet::from([
            Effect::MissionStateWrite,
            Effect::RuntimeStoreWrite,
            Effect::CoordinationRecord,
        ]),
        verified_object_digest: object_digest.to_string(),
        authorization_snapshot_digest: hash('c'),
        authority_decision_digest: Some(hash('d')),
        identity_role_binding_digest: Some(hash('e')),
        upstream_verification_receipt_digest: hash('f'),
        protected_time_evidence_digest: hash('1'),
        constitution_digest: hash('2'),
        constitution_epoch: 2,
        autonomy_epoch: 3,
        protected_epoch: 4,
        policy_registry_digest: hash('4'),
        authorization_lease_id: "test-authorization-lease".to_string(),
        authorization_reservation_id: hash('8'),
        authenticated_at: NOW - 100,
        expires_at: NOW + 10_000,
    }
}

pub(crate) fn service_decision(id: &str) -> MissionServiceDecisionV1 {
    let mut decision = MissionServiceDecisionV1 {
        schema: MISSION_SERVICE_DECISION_SCHEMA.to_string(),
        decision_id: id.to_string(),
        issuer: SERVICE_ACTOR.to_string(),
        reason_digest: hash('2'),
        decision_digest: String::new(),
    };
    decision.seal().unwrap();
    decision
}

pub(crate) fn payload(evidence: MissionTransitionEvidenceV1) -> MissionTransitionPayloadV1 {
    MissionTransitionPayloadV1 {
        schema: MISSION_TRANSITION_PAYLOAD_SCHEMA.to_string(),
        brain_id: BRAIN.to_string(),
        mission_id: MISSION.to_string(),
        block_id: BLOCK.to_string(),
        expected_store_version: 7,
        expected_boundary_version: 3,
        expected_contract_version: 2,
        evidence,
    }
}

fn evidence_source_digest(evidence: &MissionTransitionEvidenceV1) -> String {
    match evidence {
        MissionTransitionEvidenceV1::MissionServiceDecision { decision, .. } => {
            decision.decision_digest.clone()
        }
        MissionTransitionEvidenceV1::AuthorProposal { proposal, .. } => {
            proposal.proposal_digest.clone()
        }
        MissionTransitionEvidenceV1::ReviewResult { result, .. } => result.result_digest.clone(),
        MissionTransitionEvidenceV1::ExecutionDispatchAck { ack } => ack.ack_digest.clone(),
        MissionTransitionEvidenceV1::ExecutionResult { result, .. } => result.result_digest.clone(),
    }
}

#[allow(clippy::too_many_arguments)] // Test builder mirrors the versioned wire contract.
pub(crate) fn transition_intent(
    service: &MissionService,
    transition_id: &str,
    to_state: MissionState,
    actor_id: &str,
    role: Role,
    capability_id: &str,
    payload: &MissionTransitionPayloadV1,
    packet_digest: String,
    idempotency_key: &str,
) -> MissionTransitionIntentV1 {
    let current = service.head(MISSION);
    let source = match &payload.evidence {
        MissionTransitionEvidenceV1::MissionServiceDecision { .. } => {
            MissionTransitionSource::MissionServiceDecision
        }
        MissionTransitionEvidenceV1::AuthorProposal { .. } => {
            MissionTransitionSource::AuthorProposal
        }
        MissionTransitionEvidenceV1::ReviewResult { .. } => MissionTransitionSource::ReviewResult,
        MissionTransitionEvidenceV1::ExecutionDispatchAck { .. } => {
            MissionTransitionSource::ExecutionDispatchAck
        }
        MissionTransitionEvidenceV1::ExecutionResult { .. } => {
            MissionTransitionSource::ExecutionResult
        }
    };
    let rule = m1nd_control::mission_transition_rule(current.map(|head| head.state), to_state)
        .expect("test asks for a legal edge");
    assert_eq!(rule.role, role);
    assert_eq!(rule.source, source);
    let iteration_id = match (current, rule.iteration) {
        (None, _) => 1,
        (Some(head), m1nd_control::IterationRule::Preserve) => head.iteration_id,
        (Some(head), m1nd_control::IterationRule::Advance) => head.iteration_id + 1,
        (Some(_), m1nd_control::IterationRule::Initialize) => unreachable!(),
    };
    let mut intent = MissionTransitionIntentV1 {
        schema: MISSION_TRANSITION_INTENT_SCHEMA.to_string(),
        transition_id: transition_id.to_string(),
        brain_id: BRAIN.to_string(),
        mission_id: MISSION.to_string(),
        expected_head_id: current.map(|head| head.head_id.clone()),
        from_state: current.map(|head| head.state),
        to_state,
        iteration_id,
        actor_id: actor_id.to_string(),
        role,
        source,
        source_digest: evidence_source_digest(&payload.evidence),
        capability_id: capability_id.to_string(),
        packet_digest,
        payload_digest: MissionTransitionIntentV1::payload_digest_for(payload).unwrap(),
        idempotency_key: idempotency_key.to_string(),
        causation_id: None,
        issued_at: NOW - 100,
        expires_at: NOW + 10_000,
        issuer: actor_id.to_string(),
        key_id: format!("key-{actor_id}"),
        algorithm: "upstream-opaque-test".to_string(),
        intent_digest: String::new(),
        signature: OpaqueSignature::new("upstream-transition-artifact"),
    };
    intent.seal().unwrap();
    intent
}

pub(crate) fn dispatch(genesis_anchor: &str, packet_digest: &str) -> ExecutionDispatchV1 {
    let mut dispatch = ExecutionDispatchV1 {
        schema: EXECUTION_DISPATCH_SCHEMA.to_string(),
        execution_id: "exec-1".to_string(),
        brain_id: BRAIN.to_string(),
        mission_id: MISSION.to_string(),
        mission_head_id: genesis_anchor.to_string(),
        iteration_id: 1,
        packet_digest: packet_digest.to_string(),
        runner_id: RUNNER.to_string(),
        idempotency_key: "dispatch-idem-1".to_string(),
        issued_at: NOW - 50,
        deadline_at: NOW + 5_000,
        state: ExecutionDispatchState::Intent,
        issuer: SERVICE_ACTOR.to_string(),
        key_id: "owner-key-1".to_string(),
        algorithm: "upstream-opaque-test".to_string(),
        dispatch_digest: String::new(),
        signature: OpaqueSignature::new("upstream-dispatch-artifact"),
    };
    dispatch.seal().unwrap();
    dispatch
}

fn evidence_ref() -> EvidenceRefV1 {
    let mut evidence = EvidenceRefV1 {
        schema: EVIDENCE_REF_SCHEMA.to_string(),
        kind: "execution_log".to_string(),
        locator: EVIDENCE_LOCATOR.to_string(),
        sha256: hash('b'),
        producer_id: RUNNER.to_string(),
        command: Some(vec!["cargo".to_string(), "test".to_string()]),
        started_at: Some(NOW - 40),
        ended_at: Some(NOW - 20),
        retention_status: "retained".to_string(),
        evidence_digest: String::new(),
    };
    evidence.seal().unwrap();
    evidence
}

fn create_dispatching_execution(service: &mut MissionService) -> (String, ExecutionDispatchV1) {
    let packet = hash('3');
    let transition_id = "transition-dispatch-api";
    let dispatch = dispatch(transition_id, &packet);
    let transition_payload = payload(MissionTransitionEvidenceV1::MissionServiceDecision {
        decision: service_decision("decision-dispatch-api"),
        dispatch: Some(dispatch.clone()),
    });
    let intent = transition_intent(
        service,
        transition_id,
        MissionState::Dispatching,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-dispatch-api",
        &transition_payload,
        packet.clone(),
        "idem-dispatch-api",
    );
    let context = authority(
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-dispatch-api",
        &intent.intent_digest,
    );
    service
        .create_execution_dispatch(&context, &intent, &transition_payload, NOW)
        .unwrap();
    (packet, dispatch)
}

pub(crate) fn execution_ack(dispatch: &ExecutionDispatchV1) -> ExecutionDispatchAckV1 {
    let mut ack = ExecutionDispatchAckV1 {
        schema: EXECUTION_DISPATCH_ACK_SCHEMA.to_string(),
        ack_id: format!("ack-{}", dispatch.execution_id),
        execution_id: dispatch.execution_id.clone(),
        dispatch_digest: dispatch.dispatch_digest.clone(),
        brain_id: BRAIN.to_string(),
        mission_id: MISSION.to_string(),
        mission_head_id: dispatch.mission_head_id.clone(),
        iteration_id: dispatch.iteration_id,
        runner_id: RUNNER.to_string(),
        accepted_at: NOW,
        deduplicated: false,
        issuer: RUNNER.to_string(),
        key_id: "runner-key-1".to_string(),
        algorithm: "upstream-opaque-test".to_string(),
        ack_digest: String::new(),
        signature: OpaqueSignature::new("upstream-ack-artifact"),
    };
    ack.seal().unwrap();
    ack
}

pub(crate) fn successful_result_and_candidate(
    service: &MissionService,
    dispatch: &ExecutionDispatchV1,
) -> (ExecutionResultV1, ReceiptCandidateV1) {
    let executing = service.head(MISSION).unwrap();
    assert_eq!(executing.state, MissionState::Executing);
    let mut result = ExecutionResultV1 {
        schema: EXECUTION_RESULT_SCHEMA.to_string(),
        result_id: format!("result-{}", dispatch.execution_id),
        execution_id: dispatch.execution_id.clone(),
        dispatch_digest: dispatch.dispatch_digest.clone(),
        brain_id: BRAIN.to_string(),
        mission_id: MISSION.to_string(),
        mission_head_id: executing.head_id.clone(),
        iteration_id: dispatch.iteration_id,
        runner_id: RUNNER.to_string(),
        outcome: ExecutionOutcome::Succeeded,
        command: vec!["cargo".to_string(), "test".to_string()],
        exit_status: Some(0),
        started_at: NOW - 40,
        ended_at: NOW - 20,
        log_digest: hash('b'),
        failure_artifact_digest: None,
        issuer: RUNNER.to_string(),
        key_id: "runner-key-1".to_string(),
        algorithm: "upstream-opaque-test".to_string(),
        result_digest: String::new(),
        signature: OpaqueSignature::new("upstream-result-artifact"),
    };
    result.seal().unwrap();
    let mut candidate = ReceiptCandidateV1 {
        schema: RECEIPT_CANDIDATE_SCHEMA.to_string(),
        candidate_id: String::new(),
        brain_id: BRAIN.to_string(),
        mission_id: MISSION.to_string(),
        mission_head_id: executing.head_id.clone(),
        iteration_id: dispatch.iteration_id,
        block_id: BLOCK.to_string(),
        store_version: 7,
        boundary_version: 3,
        contract_version: 2,
        execution_result_digest: result.result_digest.clone(),
        receipt_type: ReceiptType::Test,
        evidence_refs: vec![evidence_ref()],
        synthetic: false,
        issuer: RUNNER.to_string(),
        key_id: "runner-key-1".to_string(),
        algorithm: "upstream-opaque-test".to_string(),
        candidate_digest: String::new(),
        signature: OpaqueSignature::new("upstream-candidate-artifact"),
    };
    candidate.seal().unwrap();
    (result, candidate)
}

pub(crate) fn advance_to_merge_wait(
    service: &mut MissionService,
    synthetic: bool,
) -> MissionLetterV1 {
    let packet = hash('3');
    let open_transition_id = "transition-open";
    let open_evidence = MissionTransitionEvidenceV1::MissionServiceDecision {
        decision: service_decision("decision-open"),
        dispatch: Some(dispatch(open_transition_id, &packet)),
    };
    let open_payload = payload(open_evidence);
    let open_intent = transition_intent(
        service,
        open_transition_id,
        MissionState::Dispatching,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-open",
        &open_payload,
        packet.clone(),
        "transition-idem-open",
    );
    let open_authority = authority(
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-open",
        &open_intent.intent_digest,
    );
    service
        .transition(&open_authority, &open_intent, &open_payload, NOW)
        .unwrap();

    let dispatch = service
        .head(MISSION)
        .unwrap()
        .execution_dispatch
        .clone()
        .unwrap();
    let mut ack = ExecutionDispatchAckV1 {
        schema: EXECUTION_DISPATCH_ACK_SCHEMA.to_string(),
        ack_id: "ack-1".to_string(),
        execution_id: dispatch.execution_id.clone(),
        dispatch_digest: dispatch.dispatch_digest.clone(),
        brain_id: BRAIN.to_string(),
        mission_id: MISSION.to_string(),
        mission_head_id: dispatch.mission_head_id.clone(),
        iteration_id: 1,
        runner_id: RUNNER.to_string(),
        accepted_at: NOW,
        deduplicated: false,
        issuer: RUNNER.to_string(),
        key_id: "runner-key-1".to_string(),
        algorithm: "upstream-opaque-test".to_string(),
        ack_digest: String::new(),
        signature: OpaqueSignature::new("upstream-ack-artifact"),
    };
    // The ACK contract binds mission_head_id to the dispatch's pre-head anchor,
    // while the transition intent binds the actual dispatching head through CAS.
    ack.seal().unwrap();
    let ack_payload = payload(MissionTransitionEvidenceV1::ExecutionDispatchAck { ack });
    let ack_intent = transition_intent(
        service,
        "transition-ack",
        MissionState::Executing,
        RUNNER,
        Role::Runner,
        "cap-runner",
        &ack_payload,
        packet.clone(),
        "transition-idem-ack",
    );
    let ack_authority = authority(
        RUNNER,
        Role::Runner,
        "cap-runner",
        &ack_intent.intent_digest,
    );
    service
        .transition(&ack_authority, &ack_intent, &ack_payload, NOW)
        .unwrap();

    let executing = service.head(MISSION).unwrap().clone();
    let dispatch = executing.execution_dispatch.clone().unwrap();
    let mut result = ExecutionResultV1 {
        schema: EXECUTION_RESULT_SCHEMA.to_string(),
        result_id: "result-1".to_string(),
        execution_id: dispatch.execution_id.clone(),
        dispatch_digest: dispatch.dispatch_digest.clone(),
        brain_id: BRAIN.to_string(),
        mission_id: MISSION.to_string(),
        mission_head_id: executing.head_id.clone(),
        iteration_id: 1,
        runner_id: RUNNER.to_string(),
        outcome: ExecutionOutcome::Succeeded,
        command: vec!["cargo".to_string(), "test".to_string()],
        exit_status: Some(0),
        started_at: NOW - 40,
        ended_at: NOW - 20,
        log_digest: hash('b'),
        failure_artifact_digest: None,
        issuer: RUNNER.to_string(),
        key_id: "runner-key-1".to_string(),
        algorithm: "upstream-opaque-test".to_string(),
        result_digest: String::new(),
        signature: OpaqueSignature::new("upstream-result-artifact"),
    };
    result.seal().unwrap();
    let mut candidate = ReceiptCandidateV1 {
        schema: RECEIPT_CANDIDATE_SCHEMA.to_string(),
        candidate_id: String::new(),
        brain_id: BRAIN.to_string(),
        mission_id: MISSION.to_string(),
        mission_head_id: executing.head_id,
        iteration_id: 1,
        block_id: BLOCK.to_string(),
        store_version: 7,
        boundary_version: 3,
        contract_version: 2,
        execution_result_digest: result.result_digest.clone(),
        receipt_type: ReceiptType::Test,
        evidence_refs: vec![evidence_ref()],
        synthetic,
        issuer: RUNNER.to_string(),
        key_id: "runner-key-1".to_string(),
        algorithm: "upstream-opaque-test".to_string(),
        candidate_digest: String::new(),
        signature: OpaqueSignature::new("upstream-candidate-artifact"),
    };
    candidate.seal().unwrap();
    let result_payload = payload(MissionTransitionEvidenceV1::ExecutionResult {
        result,
        candidate: Some(candidate),
    });
    let result_intent = transition_intent(
        service,
        "transition-result",
        MissionState::Gate,
        RUNNER,
        Role::Runner,
        "cap-runner-result",
        &result_payload,
        packet.clone(),
        "transition-idem-result",
    );
    let result_authority = authority(
        RUNNER,
        Role::Runner,
        "cap-runner-result",
        &result_intent.intent_digest,
    );
    service
        .transition(&result_authority, &result_intent, &result_payload, NOW)
        .unwrap();

    let merge_payload = payload(MissionTransitionEvidenceV1::MissionServiceDecision {
        decision: service_decision("decision-merge-wait"),
        dispatch: None,
    });
    let merge_intent = transition_intent(
        service,
        "transition-merge-wait",
        MissionState::MergeWait,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-merge-wait",
        &merge_payload,
        packet,
        "transition-idem-merge-wait",
    );
    let merge_authority = authority(
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-merge-wait",
        &merge_intent.intent_digest,
    );
    service
        .transition(&merge_authority, &merge_intent, &merge_payload, NOW)
        .unwrap()
        .letter
}

pub(crate) fn build_land(
    service: &MissionService,
    transaction_id: &str,
    idempotency_key: &str,
) -> (AuthenticatedAuthorityContextV1, LandRequestV1) {
    let head = service.head(MISSION).unwrap();
    let candidate = head.receipt_candidate.as_ref().unwrap();
    let core = service
        .canonical_land_intent(
            MISSION,
            &head.head_id,
            &candidate.candidate_id,
            &candidate.candidate_digest,
            7,
            idempotency_key,
        )
        .unwrap();
    let intent_digest = core.compute_intent_digest().unwrap();
    let mut transaction =
        AuthorityTransactionV1::PositiveAuthority(PositiveAuthorityTransactionV1 {
            schema: POSITIVE_AUTHORITY_TRANSACTION_SCHEMA.to_string(),
            binding: AuthorityTransactionBindingV1 {
                transaction_id: transaction_id.to_string(),
                organism_id: ORGANISM.to_string(),
                brain_id: BRAIN.to_string(),
                subject_id: HUMAN.to_string(),
                action_id: "land".to_string(),
                idempotency_key: idempotency_key.to_string(),
                intent_core_ref: format!("intent:{intent_digest}"),
                intent_digest: intent_digest.clone(),
                intent_canonicalization_version: CANONICALIZATION_VERSION.to_string(),
                capability_id: "cap-land-1".to_string(),
                capability_kind: CapabilityKind::Human,
                nonce: format!("nonce-{transaction_id}"),
                expected_head_id: Some(head.head_id.clone()),
                expected_active_mode: ActiveMode::HumanGated,
                expected_activation_receipt_id: None,
                expected_constitution_epoch: 2,
                expected_autonomy_epoch: 3,
                expected_store_epoch: 4,
                sentinel_verdict_digest: None,
                authorization_snapshot_digest: hash('c'),
                issued_at: NOW - 100,
                expires_at: NOW + 10_000,
            },
            authority_decision_digest: hash('d'),
            identity_role_binding_digest: hash('e'),
            required_authority_variant: AuthorityVariant::Human,
            action_policy_registry_digest: hash('4'),
            classifier_decision_digest: hash('5'),
            expected_pending_red_set_digest: hash('6'),
            expected_red_latch_epoch: 1,
            expected_store_version: 7,
            expected_boundary_version: 3,
            expected_contract_version: 2,
            action_payload_digest: intent_digest,
            issuer: "owner-1".to_string(),
            key_id: "owner-key-1".to_string(),
            algorithm: "upstream-opaque-test".to_string(),
            transaction_digest: String::new(),
            signature: OpaqueSignature::new("upstream-transaction-artifact"),
        });
    transaction.seal().unwrap();
    let mut auth = authority(
        HUMAN,
        Role::MissionService,
        "cap-land-1",
        transaction.transaction_digest(),
    );
    auth.action_id = "mission.service.land".to_string();
    auth.mission_head_id = Some(head.head_id.clone());
    auth.complete_effects.insert(Effect::SovereignMutation);
    let request = LandRequestV1 {
        schema: LAND_REQUEST_SCHEMA.to_string(),
        brain_id: BRAIN.to_string(),
        mission_id: MISSION.to_string(),
        expected_head_id: head.head_id.clone(),
        candidate_id: candidate.candidate_id.clone(),
        expected_candidate_digest: candidate.candidate_digest.clone(),
        expected_store_version: 7,
        idempotency_key: idempotency_key.to_string(),
        transaction,
    };
    (auth, request)
}

#[test]
fn execution_lifecycle_reconciles_durable_runner_snapshots_only_through_service() {
    let directory = tempfile::tempdir().unwrap();
    let mut service = open_service(&directory);
    let (packet, dispatch) = create_dispatching_execution(&mut service);
    assert_eq!(
        service.head(MISSION).unwrap().state,
        MissionState::Dispatching
    );
    assert_eq!(
        service
            .execution_dispatch(&dispatch.execution_id)
            .unwrap()
            .state,
        ExecutionDispatchState::Intent
    );

    let runner_path = directory.path().join("runner-local-inbox.jsonl");
    let mut runner = RunnerExecutionInbox::open(&runner_path, RUNNER).unwrap();
    let claim = match runner.claim_for_spawn(dispatch.clone(), NOW - 45).unwrap() {
        RunnerClaimOutcome::Spawn(permit) => permit.claim,
        RunnerClaimOutcome::AlreadyClaimed { .. } => panic!("first delivery must spawn once"),
    };
    runner
        .mark_process_started(
            &dispatch.execution_id,
            &claim.claim_id,
            "pid:41:start:900",
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
        "transition-started-snapshot",
        MissionState::Executing,
        RUNNER,
        Role::Runner,
        "cap-started-snapshot",
        &ack_payload,
        packet.clone(),
        "idem-started-snapshot",
    );
    let ack_authority = authority(
        RUNNER,
        Role::Runner,
        "cap-started-snapshot",
        &ack_intent.intent_digest,
    );
    let executing = service
        .reconcile_runner_started_snapshot(
            &ack_authority,
            &started_snapshot,
            &ack_intent,
            &ack_payload,
            NOW,
        )
        .unwrap()
        .letter;
    assert_eq!(executing.state, MissionState::Executing);
    let owner_entry = service.execution_dispatch(&dispatch.execution_id).unwrap();
    assert_eq!(owner_entry.state, ExecutionDispatchState::Acked);
    assert_eq!(
        owner_entry.executing_head.as_ref().unwrap().head_id,
        executing.head_id
    );

    runner.record_ack(ack.clone(), NOW).unwrap();
    runner
        .observe_executing_transition(
            &dispatch.execution_id,
            &ack.ack_digest,
            ExecutionMissionHeadV1 {
                schema: EXECUTION_MISSION_HEAD_SCHEMA.to_string(),
                head_id: executing.head_id.clone(),
                state: MissionState::Executing,
                iteration_id: executing.iteration_id,
                packet_digest: executing.packet_digest.clone(),
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
        candidate: Some(candidate),
    });
    let result_intent = transition_intent(
        &service,
        "transition-terminal-snapshot",
        MissionState::Gate,
        RUNNER,
        Role::Runner,
        "cap-terminal-snapshot",
        &result_payload,
        packet,
        "idem-terminal-snapshot",
    );
    let result_authority = authority(
        RUNNER,
        Role::Runner,
        "cap-terminal-snapshot",
        &result_intent.intent_digest,
    );
    let gate = service
        .reconcile_runner_terminal_snapshot(
            &result_authority,
            &terminal_snapshot,
            &result_intent,
            &result_payload,
            NOW,
        )
        .unwrap()
        .letter;
    assert_eq!(gate.state, MissionState::Gate);
    let owner_entry = service.execution_dispatch(&dispatch.execution_id).unwrap();
    assert_eq!(owner_entry.state, ExecutionDispatchState::Completed);
    assert_eq!(
        owner_entry.result_transition_head_id.as_deref(),
        Some(gate.head_id.as_str())
    );
    assert!(matches!(
        service.execution_reconciliation_actions(NOW).as_slice(),
        [OwnerReconciliationAction::Settled { .. }]
    ));

    let outbox_bytes =
        std::fs::read(directory.path().join(MISSION_SERVICE_EXECUTION_OUTBOX_FILE)).unwrap();
    let outbox_text = String::from_utf8(outbox_bytes).unwrap();
    assert!(!outbox_text.contains(MISSION_LETTER_V1_SCHEMA));
    assert!(!outbox_text.contains("\"mission_seq\""));
    assert!(!outbox_text.contains("\"authored_by\""));
    let service_state = String::from_utf8(
        std::fs::read(directory.path().join(MISSION_SERVICE_STATE_FILE)).unwrap(),
    )
    .unwrap();
    assert!(service_state.contains(MISSION_LETTER_V1_SCHEMA));
    assert!(service_state.contains("\"mission_seq\""));
}

#[test]
fn execution_lifecycle_crash_gaps_replay_without_ghost_or_duplicate_letters() {
    let directory = tempfile::tempdir().unwrap();
    let packet = hash('3');
    let transition_id = "transition-dispatch-crash";
    let dispatch = dispatch(transition_id, &packet);
    let dispatch_payload = payload(MissionTransitionEvidenceV1::MissionServiceDecision {
        decision: service_decision("decision-dispatch-crash"),
        dispatch: Some(dispatch.clone()),
    });
    let mut service = open_service(&directory);
    let dispatch_intent = transition_intent(
        &service,
        transition_id,
        MissionState::Dispatching,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-dispatch-crash",
        &dispatch_payload,
        packet.clone(),
        "idem-dispatch-crash",
    );
    let dispatch_authority = authority(
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-dispatch-crash",
        &dispatch_intent.intent_digest,
    );
    let error = service
        .transition_execution_until_crash_for_test(
            &dispatch_authority,
            &dispatch_intent,
            &dispatch_payload,
            NOW,
            ExecutionLifecycleCrashPoint::DispatchLetterPersisted,
        )
        .unwrap_err();
    assert_eq!(error.code(), "simulated_execution_lifecycle_crash");
    assert_eq!(
        service.head(MISSION).unwrap().state,
        MissionState::Dispatching
    );
    assert!(service.execution_dispatch(&dispatch.execution_id).is_none());
    drop(service);

    let mut service = open_service(&directory);
    assert_eq!(
        service
            .execution_dispatch(&dispatch.execution_id)
            .unwrap()
            .state,
        ExecutionDispatchState::Intent
    );
    assert!(matches!(
        service.execution_reconciliation_actions(NOW).as_slice(),
        [OwnerReconciliationAction::RedeliverIntent { .. }]
    ));

    let ack = execution_ack(&dispatch);
    let ack_payload =
        payload(MissionTransitionEvidenceV1::ExecutionDispatchAck { ack: ack.clone() });
    let ack_intent = transition_intent(
        &service,
        "transition-ack-crash",
        MissionState::Executing,
        RUNNER,
        Role::Runner,
        "cap-ack-crash",
        &ack_payload,
        packet.clone(),
        "idem-ack-crash",
    );
    let ack_authority = authority(
        RUNNER,
        Role::Runner,
        "cap-ack-crash",
        &ack_intent.intent_digest,
    );
    let error = service
        .transition_execution_until_crash_for_test(
            &ack_authority,
            &ack_intent,
            &ack_payload,
            NOW,
            ExecutionLifecycleCrashPoint::AckJournaled,
        )
        .unwrap_err();
    assert_eq!(error.code(), "simulated_execution_lifecycle_crash");
    assert_eq!(
        service.head(MISSION).unwrap().state,
        MissionState::Dispatching
    );
    drop(service);

    let mut service = open_service(&directory);
    assert!(matches!(
        service.execution_reconciliation_actions(NOW).as_slice(),
        [OwnerReconciliationAction::ApplyExecutingTransition { .. }]
    ));
    let error = service
        .transition_execution_until_crash_for_test(
            &ack_authority,
            &ack_intent,
            &ack_payload,
            NOW,
            ExecutionLifecycleCrashPoint::ExecutingLetterPersisted,
        )
        .unwrap_err();
    assert_eq!(error.code(), "simulated_execution_lifecycle_crash");
    assert_eq!(
        service.head(MISSION).unwrap().state,
        MissionState::Executing
    );
    drop(service);

    let mut service = open_service(&directory);
    assert!(matches!(
        service.execution_reconciliation_actions(NOW).as_slice(),
        [OwnerReconciliationAction::AwaitResult { .. }]
    ));
    let (result, candidate) = successful_result_and_candidate(&service, &dispatch);
    let result_payload = payload(MissionTransitionEvidenceV1::ExecutionResult {
        result: result.clone(),
        candidate: Some(candidate),
    });
    let result_intent = transition_intent(
        &service,
        "transition-result-crash",
        MissionState::Gate,
        RUNNER,
        Role::Runner,
        "cap-result-crash",
        &result_payload,
        packet,
        "idem-result-crash",
    );
    let result_authority = authority(
        RUNNER,
        Role::Runner,
        "cap-result-crash",
        &result_intent.intent_digest,
    );
    let error = service
        .transition_execution_until_crash_for_test(
            &result_authority,
            &result_intent,
            &result_payload,
            NOW,
            ExecutionLifecycleCrashPoint::ResultJournaled,
        )
        .unwrap_err();
    assert_eq!(error.code(), "simulated_execution_lifecycle_crash");
    assert_eq!(
        service.head(MISSION).unwrap().state,
        MissionState::Executing
    );
    drop(service);

    let mut service = open_service(&directory);
    assert!(matches!(
        service.execution_reconciliation_actions(NOW).as_slice(),
        [OwnerReconciliationAction::ApplyResultTransition { .. }]
    ));
    let error = service
        .transition_execution_until_crash_for_test(
            &result_authority,
            &result_intent,
            &result_payload,
            NOW,
            ExecutionLifecycleCrashPoint::TerminalLetterPersisted,
        )
        .unwrap_err();
    assert_eq!(error.code(), "simulated_execution_lifecycle_crash");
    assert_eq!(service.head(MISSION).unwrap().state, MissionState::Gate);
    drop(service);

    let service = open_service(&directory);
    assert!(matches!(
        service.execution_reconciliation_actions(NOW).as_slice(),
        [OwnerReconciliationAction::Settled { .. }]
    ));
    let letters: serde_json::Value = serde_json::from_slice(
        &std::fs::read(directory.path().join(MISSION_SERVICE_STATE_FILE)).unwrap(),
    )
    .unwrap();
    assert_eq!(letters["letters"].as_array().unwrap().len(), 3);
}

#[test]
fn golden_path_persists_only_owner_letters_and_lands_receipt_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let mut service = open_service(&directory);
    let merge_wait = advance_to_merge_wait(&mut service, false);
    assert_eq!(merge_wait.state, MissionState::MergeWait);
    assert!(service.receipts().is_empty());

    let (auth, request) = build_land(&service, "land-tx-1", "land-idem-1");
    assert_eq!(
        auth.authentication_disposition(),
        AuthenticationDisposition::UpstreamAuthenticationTrustedNotReverified
    );
    let outcome = service.land(&auth, &request, NOW).unwrap();
    assert!(!outcome.deduplicated);
    assert_eq!(outcome.resulting_store_version, 8);
    let head = service.head(MISSION).unwrap();
    assert_eq!(head.state, MissionState::Landed);
    assert_eq!(head.authored_by, SERVICE_ACTOR);
    assert_eq!(head.transaction_id.as_deref(), Some("land-tx-1"));
    assert_eq!(
        head.committed_receipt_id.as_deref(),
        Some(outcome.receipt_id.as_str())
    );
    assert_eq!(service.receipts().len(), 1);
    assert_eq!(
        service.receipt(&outcome.receipt_id).unwrap().mission_id,
        MISSION
    );

    let replay = service.land(&auth, &request, NOW).unwrap();
    assert!(replay.deduplicated);
    assert_eq!(replay.receipt_id, outcome.receipt_id);
    assert_eq!(service.receipts().len(), 1);

    let historical_replay_at = NOW + 20_000;
    let mut fresh_replay_authority = auth.clone();
    fresh_replay_authority.authenticated_at = historical_replay_at - 100;
    fresh_replay_authority.expires_at = historical_replay_at + 1_000;
    let historical_replay = service
        .land(&fresh_replay_authority, &request, historical_replay_at)
        .unwrap();
    assert!(historical_replay.deduplicated);
    assert_eq!(historical_replay.receipt_id, outcome.receipt_id);

    let mut altered_replay = request.clone();
    altered_replay.candidate_id = "rcd:altered-replay".to_string();
    let error = service.land(&auth, &altered_replay, NOW).unwrap_err();
    assert_eq!(error.code(), "idempotency_conflict");
    let mut expired_replay_authority = auth;
    expired_replay_authority.expires_at = NOW;
    let error = service
        .land(&expired_replay_authority, &request, NOW)
        .unwrap_err();
    assert_eq!(error.code(), "authenticated_context_expired");
    assert_eq!(service.receipts().len(), 1);
}

#[test]
fn transition_replay_requires_live_exact_authority_and_unchanged_payload() {
    let directory = tempfile::tempdir().unwrap();
    let mut service = open_service(&directory);
    let packet = hash('3');
    let transition_id = "transition-replay-auth";
    let transition_payload = payload(MissionTransitionEvidenceV1::MissionServiceDecision {
        decision: service_decision("decision-replay-auth"),
        dispatch: Some(dispatch(transition_id, &packet)),
    });
    let intent = transition_intent(
        &service,
        transition_id,
        MissionState::Dispatching,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-replay-auth",
        &transition_payload,
        packet,
        "idem-replay-auth",
    );
    let valid_authority = authority(
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-replay-auth",
        &intent.intent_digest,
    );
    let first = service
        .transition(&valid_authority, &intent, &transition_payload, NOW)
        .unwrap();
    assert!(!first.deduplicated);
    let replay = service
        .transition(&valid_authority, &intent, &transition_payload, NOW)
        .unwrap();
    assert!(replay.deduplicated);
    assert_eq!(replay.letter.head_id, first.letter.head_id);

    let mut expired = valid_authority.clone();
    expired.expires_at = NOW;
    let error = service
        .transition(&expired, &intent, &transition_payload, NOW)
        .unwrap_err();
    assert_eq!(error.code(), "authenticated_context_expired");

    let mut wrong_brain = valid_authority.clone();
    wrong_brain.brain_id = "another-brain".to_string();
    let error = service
        .transition(&wrong_brain, &intent, &transition_payload, NOW)
        .unwrap_err();
    assert_eq!(error.code(), "brain_mismatch");

    let mut altered_payload = transition_payload;
    altered_payload.expected_boundary_version += 1;
    let error = service
        .transition(&valid_authority, &intent, &altered_payload, NOW)
        .unwrap_err();
    assert_eq!(error.code(), "transition_payload_mismatch");
    assert_eq!(service.head(MISSION).unwrap().mission_seq, 1);
}

#[test]
fn invented_anchor_is_refused_before_any_gate_letter_is_persisted() {
    let directory = tempfile::tempdir().unwrap();
    let mut service = open_service(&directory);

    // Reach executing, then compose a successful result whose structurally
    // valid evidence anchor is absent from the owner's canonical catalog.
    let packet = hash('3');
    let open_id = "transition-open-invented";
    let open_payload = payload(MissionTransitionEvidenceV1::MissionServiceDecision {
        decision: service_decision("decision-open-invented"),
        dispatch: Some(dispatch(open_id, &packet)),
    });
    let open_intent = transition_intent(
        &service,
        open_id,
        MissionState::Dispatching,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-open-invented",
        &open_payload,
        packet.clone(),
        "idem-open-invented",
    );
    service
        .transition(
            &authority(
                SERVICE_ACTOR,
                Role::MissionService,
                "cap-open-invented",
                &open_intent.intent_digest,
            ),
            &open_intent,
            &open_payload,
            NOW,
        )
        .unwrap();
    let stored_dispatch = service
        .head(MISSION)
        .unwrap()
        .execution_dispatch
        .clone()
        .unwrap();
    let mut ack = ExecutionDispatchAckV1 {
        schema: EXECUTION_DISPATCH_ACK_SCHEMA.to_string(),
        ack_id: "ack-invented".to_string(),
        execution_id: stored_dispatch.execution_id.clone(),
        dispatch_digest: stored_dispatch.dispatch_digest.clone(),
        brain_id: BRAIN.to_string(),
        mission_id: MISSION.to_string(),
        mission_head_id: stored_dispatch.mission_head_id.clone(),
        iteration_id: 1,
        runner_id: RUNNER.to_string(),
        accepted_at: NOW,
        deduplicated: false,
        issuer: RUNNER.to_string(),
        key_id: "runner-key-1".to_string(),
        algorithm: "upstream-opaque-test".to_string(),
        ack_digest: String::new(),
        signature: OpaqueSignature::new("upstream-ack-artifact"),
    };
    ack.seal().unwrap();
    let ack_payload = payload(MissionTransitionEvidenceV1::ExecutionDispatchAck { ack });
    let ack_intent = transition_intent(
        &service,
        "transition-ack-invented",
        MissionState::Executing,
        RUNNER,
        Role::Runner,
        "cap-ack-invented",
        &ack_payload,
        packet.clone(),
        "idem-ack-invented",
    );
    service
        .transition(
            &authority(
                RUNNER,
                Role::Runner,
                "cap-ack-invented",
                &ack_intent.intent_digest,
            ),
            &ack_intent,
            &ack_payload,
            NOW,
        )
        .unwrap();
    let executing = service.head(MISSION).unwrap().clone();
    let dispatch = executing.execution_dispatch.clone().unwrap();
    let mut result = ExecutionResultV1 {
        schema: EXECUTION_RESULT_SCHEMA.to_string(),
        result_id: "result-invented".to_string(),
        execution_id: dispatch.execution_id.clone(),
        dispatch_digest: dispatch.dispatch_digest.clone(),
        brain_id: BRAIN.to_string(),
        mission_id: MISSION.to_string(),
        mission_head_id: executing.head_id.clone(),
        iteration_id: 1,
        runner_id: RUNNER.to_string(),
        outcome: ExecutionOutcome::Succeeded,
        command: vec!["cargo".to_string(), "test".to_string()],
        exit_status: Some(0),
        started_at: NOW - 40,
        ended_at: NOW - 20,
        log_digest: hash('b'),
        failure_artifact_digest: None,
        issuer: RUNNER.to_string(),
        key_id: "runner-key-1".to_string(),
        algorithm: "upstream-opaque-test".to_string(),
        result_digest: String::new(),
        signature: OpaqueSignature::new("upstream-result-artifact"),
    };
    result.seal().unwrap();
    let mut invented = evidence_ref();
    invented.locator = "proofs/invented.log".to_string();
    invented.sha256 = hash('9');
    invented.seal().unwrap();
    let mut candidate = ReceiptCandidateV1 {
        schema: RECEIPT_CANDIDATE_SCHEMA.to_string(),
        candidate_id: String::new(),
        brain_id: BRAIN.to_string(),
        mission_id: MISSION.to_string(),
        mission_head_id: executing.head_id.clone(),
        iteration_id: 1,
        block_id: BLOCK.to_string(),
        store_version: 7,
        boundary_version: 3,
        contract_version: 2,
        execution_result_digest: result.result_digest.clone(),
        receipt_type: ReceiptType::Test,
        evidence_refs: vec![invented],
        synthetic: false,
        issuer: RUNNER.to_string(),
        key_id: "runner-key-1".to_string(),
        algorithm: "upstream-opaque-test".to_string(),
        candidate_digest: String::new(),
        signature: OpaqueSignature::new("upstream-candidate-artifact"),
    };
    candidate.seal().unwrap();
    let result_payload = payload(MissionTransitionEvidenceV1::ExecutionResult {
        result,
        candidate: Some(candidate),
    });
    let result_intent = transition_intent(
        &service,
        "transition-result-invented",
        MissionState::Gate,
        RUNNER,
        Role::Runner,
        "cap-result-invented",
        &result_payload,
        packet,
        "idem-result-invented",
    );
    let error = service
        .transition(
            &authority(
                RUNNER,
                Role::Runner,
                "cap-result-invented",
                &result_intent.intent_digest,
            ),
            &result_intent,
            &result_payload,
            NOW,
        )
        .unwrap_err();
    assert_eq!(error.code(), "invented_evidence_anchor");
    assert_eq!(service.head(MISSION).unwrap().head_id, executing.head_id);
}

#[test]
fn illegal_transition_and_wrong_author_are_refused_without_persistence() {
    let directory = tempfile::tempdir().unwrap();
    let mut service = open_service(&directory);
    let packet = hash('3');
    let illegal_payload = payload(MissionTransitionEvidenceV1::MissionServiceDecision {
        decision: service_decision("decision-illegal"),
        dispatch: None,
    });
    let mut illegal = MissionTransitionIntentV1 {
        schema: MISSION_TRANSITION_INTENT_SCHEMA.to_string(),
        transition_id: "transition-illegal".to_string(),
        brain_id: BRAIN.to_string(),
        mission_id: MISSION.to_string(),
        expected_head_id: None,
        from_state: None,
        to_state: MissionState::Landed,
        iteration_id: 1,
        actor_id: SERVICE_ACTOR.to_string(),
        role: Role::MissionService,
        source: MissionTransitionSource::MissionServiceDecision,
        source_digest: evidence_source_digest(&illegal_payload.evidence),
        capability_id: "cap-illegal".to_string(),
        packet_digest: packet.clone(),
        payload_digest: MissionTransitionIntentV1::payload_digest_for(&illegal_payload).unwrap(),
        idempotency_key: "idem-illegal".to_string(),
        causation_id: None,
        issued_at: NOW - 100,
        expires_at: NOW + 10_000,
        issuer: SERVICE_ACTOR.to_string(),
        key_id: "owner-key-1".to_string(),
        algorithm: "upstream-opaque-test".to_string(),
        intent_digest: String::new(),
        signature: OpaqueSignature::new("upstream-transition-artifact"),
    };
    illegal.seal().unwrap();
    let error = service
        .transition(
            &authority(
                SERVICE_ACTOR,
                Role::MissionService,
                "cap-illegal",
                &illegal.intent_digest,
            ),
            &illegal,
            &illegal_payload,
            NOW,
        )
        .unwrap_err();
    assert!(error.to_string().contains("illegal mission transition"));
    assert!(service.head(MISSION).is_none());

    let open_id = "transition-wrong-author";
    let open_payload = payload(MissionTransitionEvidenceV1::MissionServiceDecision {
        decision: service_decision("decision-wrong-author"),
        dispatch: Some(dispatch(open_id, &packet)),
    });
    let open_intent = transition_intent(
        &service,
        open_id,
        MissionState::Dispatching,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-wrong-author",
        &open_payload,
        packet,
        "idem-wrong-author",
    );
    let error = service
        .transition(
            &authority(
                "impostor",
                Role::MissionService,
                "cap-wrong-author",
                &open_intent.intent_digest,
            ),
            &open_intent,
            &open_payload,
            NOW,
        )
        .unwrap_err();
    assert_eq!(error.code(), "wrong_author");
    assert!(service.head(MISSION).is_none());
}

#[test]
fn wrong_mission_block_boundary_and_contract_are_refused() {
    for (case, mutate, expected_code) in [
        ("mission", 0_u8, "mission_mismatch"),
        ("block", 1_u8, "unknown_block"),
        ("boundary", 2_u8, "stale_boundary"),
        ("contract", 3_u8, "stale_contract"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut service = open_service(&directory);
        let transition_id = format!("transition-{case}");
        let packet = hash('3');
        let mut test_payload = payload(MissionTransitionEvidenceV1::MissionServiceDecision {
            decision: service_decision(&format!("decision-{case}")),
            dispatch: Some(dispatch(&transition_id, &packet)),
        });
        match mutate {
            0 => test_payload.mission_id = "other-mission".to_string(),
            1 => test_payload.block_id = "invented-block".to_string(),
            2 => test_payload.expected_boundary_version = 99,
            3 => test_payload.expected_contract_version = 99,
            _ => unreachable!(),
        }
        let intent = transition_intent(
            &service,
            &transition_id,
            MissionState::Dispatching,
            SERVICE_ACTOR,
            Role::MissionService,
            &format!("cap-{case}"),
            &test_payload,
            packet,
            &format!("idem-{case}"),
        );
        let error = service
            .transition(
                &authority(
                    SERVICE_ACTOR,
                    Role::MissionService,
                    &format!("cap-{case}"),
                    &intent.intent_digest,
                ),
                &intent,
                &test_payload,
                NOW,
            )
            .unwrap_err();
        assert_eq!(error.code(), expected_code, "case {case}: {error}");
        assert!(service.head(MISSION).is_none());
    }
}

#[test]
fn stale_head_candidate_and_synthetic_candidate_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let mut service = open_service(&directory);
    let head = advance_to_merge_wait(&mut service, false);
    let (auth, request) = build_land(&service, "land-stale", "idem-land-stale");

    let mut stale_head = request.clone();
    stale_head.expected_head_id = "mlt:stale".to_string();
    let error = service.land(&auth, &stale_head, NOW).unwrap_err();
    assert_eq!(error.code(), "stale_head");
    assert_eq!(service.head(MISSION).unwrap().head_id, head.head_id);

    let mut stale_candidate = request;
    stale_candidate.expected_candidate_digest = hash('9');
    let error = service.land(&auth, &stale_candidate, NOW).unwrap_err();
    assert_eq!(error.code(), "candidate_mismatch");
    assert!(service.receipts().is_empty());

    let synthetic_directory = tempfile::tempdir().unwrap();
    let mut synthetic_service = open_service(&synthetic_directory);
    advance_to_merge_wait(&mut synthetic_service, true);
    let (synthetic_auth, synthetic_request) =
        build_land(&synthetic_service, "land-synthetic", "idem-land-synthetic");
    let error = synthetic_service
        .land(&synthetic_auth, &synthetic_request, NOW)
        .unwrap_err();
    assert_eq!(error.code(), "unlandable_candidate");
    assert!(synthetic_service.receipts().is_empty());
}

#[test]
fn external_legacy_writes_are_refused_regardless_of_capability() {
    let privileged = authority(SERVICE_ACTOR, Role::MissionService, "cap-any", &hash('a'));
    for ingress in [
        LegacyMutationIngress::RawMissionPost,
        LegacyMutationIngress::ReceiptImport,
        LegacyMutationIngress::RawLanded,
    ] {
        for context in [None, Some(&privileged)] {
            let error = refuse_external_legacy_mutation(ingress, context).unwrap_err();
            assert_eq!(error.code(), "legacy_direct_mutation_refused");
            assert!(error.to_string().contains("MissionService"));
        }
    }
}

#[test]
fn concurrent_seq_plus_one_has_one_winner_and_no_fork() {
    let directory = tempfile::tempdir().unwrap();
    let service = open_service(&directory);
    let packet = hash('3');
    let mut jobs = Vec::new();
    for suffix in ["a", "b"] {
        let transition_id = format!("transition-concurrent-{suffix}");
        let capability = format!("cap-concurrent-{suffix}");
        let transition_payload = payload(MissionTransitionEvidenceV1::MissionServiceDecision {
            decision: service_decision(&format!("decision-concurrent-{suffix}")),
            dispatch: Some(dispatch(&transition_id, &packet)),
        });
        let intent = transition_intent(
            &service,
            &transition_id,
            MissionState::Dispatching,
            SERVICE_ACTOR,
            Role::MissionService,
            &capability,
            &transition_payload,
            packet.clone(),
            &format!("idem-concurrent-{suffix}"),
        );
        let context = authority(
            SERVICE_ACTOR,
            Role::MissionService,
            &capability,
            &intent.intent_digest,
        );
        jobs.push((context, intent, transition_payload));
    }
    drop(service);

    // AuthorityWal's process lock is deliberately thread-affine. The owner
    // architecture therefore keeps MissionService inside one actor thread and
    // lets concurrent callers enqueue work; the service itself never crosses a
    // thread boundary.
    type ActorJob = Box<dyn FnOnce(&mut MissionService) + Send>;
    let (actor_tx, actor_rx) = mpsc::channel::<ActorJob>();
    let actor_root = directory.path().to_path_buf();
    let actor = std::thread::spawn(move || {
        let mut service =
            MissionService::open_software_test_not_production(actor_root, config()).unwrap();
        for job in actor_rx {
            job(&mut service);
        }
        let head = service.head(MISSION).unwrap();
        (head.mission_seq, head.previous_head_id.clone())
    });
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for (context, intent, transition_payload) in jobs {
        let actor_tx = actor_tx.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            let (reply_tx, reply_rx) = mpsc::channel();
            actor_tx
                .send(Box::new(move |service| {
                    let result = service.transition(&context, &intent, &transition_payload, NOW);
                    reply_tx.send(result).unwrap();
                }))
                .unwrap();
            reply_rx.recv().unwrap()
        }));
    }
    barrier.wait();
    let outcomes: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes.iter().filter(|outcome| outcome.is_err()).count(),
        1
    );
    drop(actor_tx);
    let (mission_seq, previous_head_id) = actor.join().unwrap();
    assert_eq!(mission_seq, 1);
    assert!(previous_head_id.is_none());
}

#[test]
fn wal_phase_crashes_recover_old_or_new_and_never_half_visible() {
    for (point, should_commit) in [
        (LandCrashPoint::AfterPrepare, false),
        (LandCrashPoint::AfterProvisional, false),
        (LandCrashPoint::AfterCommit, true),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut service = open_service(&directory);
        let merge_wait = advance_to_merge_wait(&mut service, false);
        let (auth, request) = build_land(
            &service,
            &format!("land-crash-{point:?}"),
            &format!("idem-crash-{point:?}"),
        );
        let error = service
            .land_until_crash_for_test(&auth, &request, NOW, point)
            .unwrap_err();
        assert_eq!(error.code(), "simulated_crash");
        if point != LandCrashPoint::AfterCommit {
            assert_eq!(service.head(MISSION).unwrap().head_id, merge_wait.head_id);
            assert!(service.receipts().is_empty());
        }
        drop(service);

        let mut recovered = open_service(&directory);
        if should_commit {
            assert_eq!(recovered.head(MISSION).unwrap().state, MissionState::Landed);
            assert_eq!(recovered.receipts().len(), 1);
            assert_eq!(recovered.recovery_report().committed_forward_completed, 1);
        } else {
            assert_eq!(
                recovered.head(MISSION).unwrap().state,
                MissionState::MergeWait
            );
            assert!(recovered.receipts().is_empty());
            assert_eq!(recovered.recovery_report().uncommitted_aborted, 1);

            let retry_at = NOW + 20_000;
            let mut retry_authority = auth.clone();
            retry_authority.authenticated_at = retry_at - 100;
            retry_authority.expires_at = retry_at + 10_000;
            let wal_path = directory.path().join("authority.wal.jsonl");
            let wal_before_retry = std::fs::read(&wal_path).unwrap();
            let first_abort = recovered
                .land(&retry_authority, &request, retry_at)
                .unwrap_err();
            assert_eq!(first_abort.code(), "transaction_previously_aborted");

            retry_authority.authenticated_at += 100;
            retry_authority.expires_at += 100;
            let second_abort = recovered
                .land(&retry_authority, &request, retry_at + 100)
                .unwrap_err();
            assert_eq!(second_abort.code(), "transaction_previously_aborted");
            assert_eq!(second_abort.to_string(), first_abort.to_string());

            let mut altered_request = request.clone();
            altered_request.candidate_id = "rcd:changed-after-abort".to_string();
            let altered = recovered
                .land(&retry_authority, &altered_request, retry_at + 100)
                .unwrap_err();
            assert_eq!(altered.code(), "idempotency_conflict");

            let mut altered_transaction_request = request.clone();
            let AuthorityTransactionV1::PositiveAuthority(altered_transaction) =
                &mut altered_transaction_request.transaction
            else {
                unreachable!()
            };
            altered_transaction.classifier_decision_digest = hash('8');
            altered_transaction_request.transaction.seal().unwrap();
            let mut altered_transaction_authority = retry_authority.clone();
            altered_transaction_authority.verified_object_digest = altered_transaction_request
                .transaction
                .transaction_digest()
                .to_string();
            let altered_transaction = recovered
                .land(
                    &altered_transaction_authority,
                    &altered_transaction_request,
                    retry_at + 100,
                )
                .unwrap_err();
            assert_eq!(altered_transaction.code(), "idempotency_conflict");

            let mut wrong_authority = retry_authority.clone();
            wrong_authority.subject_id = "other-subject".to_string();
            let wrong_author = recovered
                .land(&wrong_authority, &request, retry_at + 100)
                .unwrap_err();
            assert_eq!(wrong_author.code(), "land_authority_binding_mismatch");

            assert_eq!(std::fs::read(&wal_path).unwrap(), wal_before_retry);
            assert_eq!(recovered.head(MISSION).unwrap().head_id, merge_wait.head_id);
            assert!(recovered.receipts().is_empty());
        }
    }
}

#[test]
fn durable_plan_publish_cleans_partial_temp_and_ignores_crash_orphans() {
    let directory = tempfile::tempdir().unwrap();
    let plans = directory.path().join("land-plans");
    let unpublished = plans.join("partial-plan.json");
    let error = write_json_new_durable_partial_failure_for_test(
        &unpublished,
        &serde_json::json!({"plan": "must-not-publish"}),
    )
    .unwrap_err();
    assert!(error.to_string().contains("injected partial write failure"));
    assert!(!unpublished.exists());
    assert_eq!(std::fs::read_dir(&plans).unwrap().count(), 0);

    let orphan = plans.join(".crashed-plan.1.tmp");
    std::fs::write(&orphan, b"{\"partial\":").unwrap();
    let mut service = open_service(&directory);
    advance_to_merge_wait(&mut service, false);
    let (authority, request) = build_land(&service, "land-durable", "idem-land-durable");
    let plan_path = plans.join(format!("{}.json", request.transaction.transaction_digest()));
    service.land(&authority, &request, NOW).unwrap();

    let plan_bytes = std::fs::read(&plan_path).unwrap();
    let plan: serde_json::Value = serde_json::from_slice(&plan_bytes).unwrap();
    assert_eq!(plan["schema"], LAND_PROVISIONAL_PLAN_SCHEMA);
    assert_eq!(std::fs::read(&orphan).unwrap(), b"{\"partial\":");
    let temporary_paths = std::fs::read_dir(&plans)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("tmp"))
        .collect::<Vec<_>>();
    assert_eq!(temporary_paths, vec![orphan]);
}

#[test]
fn land_archive_reconcile_race_has_one_terminal_winner() {
    let directory = tempfile::tempdir().unwrap();
    let mut service = open_service(&directory);
    let merge_wait = advance_to_merge_wait(&mut service, false);
    let (land_authority, land_request) = build_land(&service, "land-race", "idem-land-race");

    let archive_payload = payload(MissionTransitionEvidenceV1::MissionServiceDecision {
        decision: service_decision("decision-archive-race"),
        dispatch: None,
    });
    let archive_intent = transition_intent(
        &service,
        "transition-archive-race",
        MissionState::Archived,
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-archive-race",
        &archive_payload,
        merge_wait.packet_digest.clone(),
        "idem-archive-race",
    );
    let archive_authority = authority(
        SERVICE_ACTOR,
        Role::MissionService,
        "cap-archive-race",
        &archive_intent.intent_digest,
    );

    drop(service);

    type ActorJob = Box<dyn FnOnce(&mut MissionService) + Send>;
    let (actor_tx, actor_rx) = mpsc::channel::<ActorJob>();
    let actor_root = directory.path().to_path_buf();
    let actor = std::thread::spawn(move || {
        let mut service =
            MissionService::open_software_test_not_production(actor_root, config()).unwrap();
        for job in actor_rx {
            job(&mut service);
        }
        let head = service.head(MISSION).unwrap();
        (head.state, head.mission_seq, service.receipts().len())
    });

    let barrier = Arc::new(Barrier::new(4));
    let land_handle = {
        let actor_tx = actor_tx.clone();
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            let (reply_tx, reply_rx) = mpsc::channel();
            actor_tx
                .send(Box::new(move |service| {
                    let result = service
                        .land(&land_authority, &land_request, NOW)
                        .map(|_| MissionState::Landed);
                    reply_tx.send(result).unwrap();
                }))
                .unwrap();
            reply_rx.recv().unwrap()
        })
    };
    let archive_handle = {
        let actor_tx = actor_tx.clone();
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            let (reply_tx, reply_rx) = mpsc::channel();
            actor_tx
                .send(Box::new(move |service| {
                    let result = service
                        .transition(&archive_authority, &archive_intent, &archive_payload, NOW)
                        .map(|_| MissionState::Archived);
                    reply_tx.send(result).unwrap();
                }))
                .unwrap();
            reply_rx.recv().unwrap()
        })
    };
    let reconcile_handle = {
        let actor_tx = actor_tx.clone();
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            let (reply_tx, reply_rx) = mpsc::channel();
            actor_tx
                .send(Box::new(move |service| {
                    reply_tx.send(service.reconcile(NOW)).unwrap();
                }))
                .unwrap();
            reply_rx.recv().unwrap()
        })
    };
    barrier.wait();
    let land = land_handle.join().unwrap();
    let archive = archive_handle.join().unwrap();
    reconcile_handle.join().unwrap().unwrap();

    assert_eq!(usize::from(land.is_ok()) + usize::from(archive.is_ok()), 1);
    drop(actor_tx);
    let (state, mission_seq, receipt_count) = actor.join().unwrap();
    assert!(matches!(
        state,
        MissionState::Landed | MissionState::Archived
    ));
    assert_eq!(mission_seq, merge_wait.mission_seq + 1);
    match state {
        MissionState::Landed => assert_eq!(receipt_count, 1),
        MissionState::Archived => assert_eq!(receipt_count, 0),
        _ => unreachable!(),
    }
}
