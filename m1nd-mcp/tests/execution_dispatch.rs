#[path = "../src/execution_dispatch.rs"]
mod execution_dispatch;

use std::fs;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use execution_dispatch::*;
use m1nd_control::{
    ExecutionDispatchAckV1, ExecutionDispatchState, ExecutionDispatchV1, ExecutionOutcome,
    ExecutionResultV1, OpaqueSignature, EXECUTION_DISPATCH_ACK_SCHEMA, EXECUTION_DISPATCH_SCHEMA,
    EXECUTION_RESULT_SCHEMA,
};

const BASE: u64 = 1_000_000;

fn dispatch(execution_id: &str) -> ExecutionDispatchV1 {
    let mut dispatch = ExecutionDispatchV1 {
        schema: EXECUTION_DISPATCH_SCHEMA.to_string(),
        execution_id: execution_id.to_string(),
        brain_id: "brain-a".to_string(),
        mission_id: "mission-a".to_string(),
        mission_head_id: "head-dispatching".to_string(),
        iteration_id: 3,
        packet_digest: "a".repeat(64),
        runner_id: "runner-a".to_string(),
        idempotency_key: format!("idem-{execution_id}"),
        issued_at: BASE,
        deadline_at: BASE + 10_000,
        state: ExecutionDispatchState::Intent,
        issuer: "mission-service".to_string(),
        key_id: "key-owner".to_string(),
        algorithm: "opaque-test".to_string(),
        dispatch_digest: String::new(),
        signature: OpaqueSignature::new("opaque-owner-signature"),
    };
    dispatch.seal().expect("seal dispatch");
    dispatch
}

fn ack(dispatch: &ExecutionDispatchV1) -> ExecutionDispatchAckV1 {
    let mut ack = ExecutionDispatchAckV1 {
        schema: EXECUTION_DISPATCH_ACK_SCHEMA.to_string(),
        ack_id: format!("ack-{}", dispatch.execution_id),
        execution_id: dispatch.execution_id.clone(),
        dispatch_digest: dispatch.dispatch_digest.clone(),
        brain_id: dispatch.brain_id.clone(),
        mission_id: dispatch.mission_id.clone(),
        mission_head_id: dispatch.mission_head_id.clone(),
        iteration_id: dispatch.iteration_id,
        runner_id: dispatch.runner_id.clone(),
        accepted_at: BASE + 3,
        deduplicated: false,
        issuer: dispatch.runner_id.clone(),
        key_id: "key-runner".to_string(),
        algorithm: "opaque-test".to_string(),
        ack_digest: String::new(),
        signature: OpaqueSignature::new("opaque-runner-signature"),
    };
    ack.seal().expect("seal ACK");
    ack
}

fn executing_head() -> ExecutionMissionHeadV1 {
    ExecutionMissionHeadV1 {
        schema: EXECUTION_MISSION_HEAD_SCHEMA.to_string(),
        head_id: "head-executing".to_string(),
        state: m1nd_control::MissionState::Executing,
        iteration_id: 3,
        packet_digest: "a".repeat(64),
    }
}

fn result(dispatch: &ExecutionDispatchV1) -> ExecutionResultV1 {
    let mut result = ExecutionResultV1 {
        schema: EXECUTION_RESULT_SCHEMA.to_string(),
        result_id: format!("result-{}", dispatch.execution_id),
        execution_id: dispatch.execution_id.clone(),
        dispatch_digest: dispatch.dispatch_digest.clone(),
        brain_id: dispatch.brain_id.clone(),
        mission_id: dispatch.mission_id.clone(),
        mission_head_id: executing_head().head_id,
        iteration_id: dispatch.iteration_id,
        runner_id: dispatch.runner_id.clone(),
        outcome: ExecutionOutcome::Succeeded,
        command: vec!["/usr/bin/true".to_string()],
        exit_status: Some(0),
        started_at: BASE + 2,
        ended_at: BASE + 7,
        log_digest: "b".repeat(64),
        failure_artifact_digest: None,
        issuer: dispatch.runner_id.clone(),
        key_id: "key-runner".to_string(),
        algorithm: "opaque-test".to_string(),
        result_digest: String::new(),
        signature: OpaqueSignature::new("opaque-result-signature"),
    };
    result.seal().expect("seal result");
    result
}

fn spawn_claim(outcome: RunnerClaimOutcome) -> ProcessClaimV1 {
    match outcome {
        RunnerClaimOutcome::Spawn(permit) => permit.claim,
        RunnerClaimOutcome::AlreadyClaimed { .. } => panic!("expected first spawn permit"),
    }
}

#[test]
fn golden_local_flow_is_durable_typed_and_letter_free() {
    let temp = tempfile::tempdir().expect("tempdir");
    let owner_path = temp.path().join("owner.jsonl");
    let runner_path = temp.path().join("runner.jsonl");
    let dispatch = dispatch("exec-golden");
    let ack = ack(&dispatch);
    let head = executing_head();
    let result = result(&dispatch);

    let mut owner = OwnerExecutionOutbox::open(&owner_path).expect("owner open");
    assert!(owner.is_empty());
    assert_eq!(
        owner
            .register_intent(dispatch.clone(), BASE + 1)
            .expect("owner intent"),
        OwnerIntentRegistration::Registered
    );
    assert!(matches!(
        owner.reconcile(BASE + 1).as_slice(),
        [OwnerReconciliationAction::RedeliverIntent { .. }]
    ));

    let mut runner = RunnerExecutionInbox::open(&runner_path, "runner-a").expect("runner open");
    assert!(runner.is_empty());
    assert!(RunnerInboxState::Completed.is_terminal());
    let claim = spawn_claim(
        runner
            .claim_for_spawn(dispatch.clone(), BASE + 1)
            .expect("runner claim"),
    );
    let bytes_after_claim = fs::read(&runner_path).expect("runner bytes");
    assert!(matches!(
        runner
            .claim_for_spawn(dispatch.clone(), BASE + 1)
            .expect("exact retry"),
        RunnerClaimOutcome::AlreadyClaimed {
            state: RunnerInboxState::Claimed,
            ..
        }
    ));
    assert_eq!(bytes_after_claim, fs::read(&runner_path).expect("bytes"));

    runner
        .mark_process_started(
            &dispatch.execution_id,
            &claim.claim_id,
            "pid:42:start:99",
            BASE + 2,
        )
        .expect("process started");
    runner
        .get(&dispatch.execution_id)
        .expect("started snapshot")
        .validate_for_service()
        .expect("service-facing snapshot validation");
    assert!(matches!(
        runner.reconcile().as_slice(),
        [RunnerReconciliationAction::AcceptanceAckRequired { .. }]
    ));
    runner
        .record_ack(ack.clone(), BASE + 3)
        .expect("runner ACK");
    owner.record_ack(ack.clone(), BASE + 3).expect("owner ACK");
    assert!(matches!(
        owner.reconcile(BASE + 3).as_slice(),
        [OwnerReconciliationAction::ApplyExecutingTransition { .. }]
    ));
    let owner_bytes = fs::read(&owner_path).expect("owner bytes");
    let _ = owner.reconcile(BASE + 3);
    assert_eq!(owner_bytes, fs::read(&owner_path).expect("owner bytes"));

    owner
        .mark_executing_transition(
            &dispatch.execution_id,
            &ack.ack_digest,
            head.clone(),
            BASE + 4,
        )
        .expect("owner executing marker");
    runner
        .observe_executing_transition(&dispatch.execution_id, &ack.ack_digest, head, BASE + 4)
        .expect("runner executing observation");
    assert!(matches!(
        runner.reconcile().as_slice(),
        [RunnerReconciliationAction::ObserveProcess { .. }]
    ));

    runner
        .record_result(result.clone(), BASE + 7)
        .expect("runner result");
    owner
        .record_result(result.clone(), BASE + 7)
        .expect("owner result");
    assert!(matches!(
        owner.reconcile(BASE + 7).as_slice(),
        [OwnerReconciliationAction::ApplyResultTransition {
            target_state: m1nd_control::MissionState::Gate,
            ..
        }]
    ));
    owner
        .mark_result_transition_applied(
            &dispatch.execution_id,
            &result.result_digest,
            "head-gate",
            BASE + 8,
        )
        .expect("result transition marker");
    assert!(matches!(
        owner.reconcile(BASE + 8).as_slice(),
        [OwnerReconciliationAction::Settled {
            state: ExecutionDispatchState::Completed,
            ..
        }]
    ));
    assert!(matches!(
        runner.reconcile().as_slice(),
        [RunnerReconciliationAction::DeliverResult { .. }]
    ));
    drop(owner);
    drop(runner);

    let owner = OwnerExecutionOutbox::open(&owner_path).expect("owner restart");
    let runner = RunnerExecutionInbox::open(&runner_path, "runner-a").expect("runner restart");
    assert_eq!(owner.len(), 1);
    assert_eq!(runner.len(), 1);
    assert_eq!(
        owner
            .get(&dispatch.execution_id)
            .expect("owner entry")
            .state,
        ExecutionDispatchState::Completed
    );
    assert_eq!(
        runner
            .get(&dispatch.execution_id)
            .expect("runner entry")
            .state,
        RunnerInboxState::Completed
    );
}

#[test]
fn concurrent_exact_delivery_grants_one_spawn_permit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("runner-race.jsonl");
    let inbox = Arc::new(Mutex::new(
        RunnerExecutionInbox::open(&path, "runner-a").expect("runner open"),
    ));
    let barrier = Arc::new(Barrier::new(16));
    let dispatch = dispatch("exec-race");
    let mut handles = Vec::new();
    for _ in 0..16 {
        let inbox = Arc::clone(&inbox);
        let barrier = Arc::clone(&barrier);
        let dispatch = dispatch.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            inbox
                .lock()
                .expect("mutex")
                .claim_for_spawn(dispatch, BASE + 1)
                .expect("claim")
        }));
    }
    let outcomes: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().expect("thread"))
        .collect();
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RunnerClaimOutcome::Spawn(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RunnerClaimOutcome::AlreadyClaimed { .. }))
            .count(),
        15
    );
    drop(outcomes);
    let inbox = match Arc::try_unwrap(inbox) {
        Ok(inbox) => inbox.into_inner().expect("mutex"),
        Err(_) => panic!("expected one inbox owner"),
    };
    assert_eq!(inbox.len(), 1);
    drop(inbox);
    let reopened = RunnerExecutionInbox::open(&path, "runner-a").expect("restart");
    assert_eq!(reopened.len(), 1);
}

#[test]
fn runner_refuses_identity_collisions_wrong_runner_and_ack_before_started() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("runner-identities.jsonl");
    let mut inbox = RunnerExecutionInbox::open(&path, "runner-a").expect("open");
    let original = dispatch("exec-original");
    let claim = spawn_claim(
        inbox
            .claim_for_spawn(original.clone(), BASE + 1)
            .expect("first claim"),
    );

    let mut same_execution = original.clone();
    same_execution.idempotency_key = "different-idempotency".to_string();
    same_execution.seal().expect("reseal");
    assert_eq!(
        inbox
            .claim_for_spawn(same_execution, BASE + 1)
            .expect_err("execution collision")
            .code(),
        "dispatch_identity_conflict"
    );

    let mut same_idempotency = dispatch("exec-new");
    same_idempotency.idempotency_key = original.idempotency_key.clone();
    same_idempotency.packet_digest = "c".repeat(64);
    same_idempotency.seal().expect("reseal");
    assert_eq!(
        inbox
            .claim_for_spawn(same_idempotency, BASE + 1)
            .expect_err("idempotency collision")
            .code(),
        "dispatch_identity_conflict"
    );

    let same_packet = dispatch("exec-same-packet");
    assert_eq!(
        inbox
            .claim_for_spawn(same_packet, BASE + 1)
            .expect_err("packet binding collision")
            .code(),
        "dispatch_identity_conflict"
    );

    let mut wrong_runner = dispatch("exec-wrong-runner");
    wrong_runner.runner_id = "runner-b".to_string();
    wrong_runner.seal().expect("reseal");
    assert_eq!(
        inbox
            .claim_for_spawn(wrong_runner, BASE + 1)
            .expect_err("wrong runner")
            .code(),
        "wrong_dispatch_runner"
    );

    assert_eq!(
        inbox
            .record_ack(ack(&original), BASE + 3)
            .expect_err("ACK before STARTED")
            .code(),
        "illegal_runner_inbox_transition"
    );
    inbox
        .mark_process_started(
            &original.execution_id,
            &claim.claim_id,
            "pid:wrong-ack-test",
            BASE + 2,
        )
        .expect("start process");
    let mut wrong_runner_ack = ack(&original);
    wrong_runner_ack.runner_id = "runner-b".to_string();
    wrong_runner_ack.issuer = "runner-b".to_string();
    wrong_runner_ack.seal().expect("reseal wrong runner ACK");
    assert_eq!(
        inbox
            .record_ack(wrong_runner_ack, BASE + 3)
            .expect_err("wrong runner ACK")
            .code(),
        "dispatch_contract_refused"
    );
    assert!(matches!(
        inbox.reconcile().as_slice(),
        [RunnerReconciliationAction::AcceptanceAckRequired { .. }]
    ));
}

#[test]
fn runner_claim_crash_never_reissues_spawn_or_ghosts_ack() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("claim-crash.jsonl");
    let dispatch = dispatch("exec-claim-crash");
    let mut inbox = RunnerExecutionInbox::open(&path, "runner-a").expect("open");
    assert!(matches!(
        inbox.claim_for_spawn_with_failpoint(
            dispatch.clone(),
            BASE + 1,
            Some(DispatchFailpoint::RunnerClaim),
        ),
        Err(ExecutionDispatchError::SimulatedCrash {
            point: DispatchFailpoint::RunnerClaim
        })
    ));
    drop(inbox);

    let mut inbox = RunnerExecutionInbox::open(&path, "runner-a").expect("restart");
    assert!(matches!(
        inbox
            .claim_for_spawn(dispatch.clone(), BASE + 1)
            .expect("retry"),
        RunnerClaimOutcome::AlreadyClaimed {
            state: RunnerInboxState::Claimed,
            ..
        }
    ));
    assert!(matches!(
        inbox.reconcile().as_slice(),
        [RunnerReconciliationAction::ClaimStalledNoRespawn { .. }]
    ));
    assert_eq!(
        inbox
            .record_ack(ack(&dispatch), BASE + 3)
            .expect_err("no ghost ACK")
            .code(),
        "illegal_runner_inbox_transition"
    );
}

#[test]
fn runner_started_crash_requires_ack_but_never_fabricates_one() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("started-crash.jsonl");
    let dispatch = dispatch("exec-started-crash");
    let mut inbox = RunnerExecutionInbox::open(&path, "runner-a").expect("open");
    let claim = spawn_claim(
        inbox
            .claim_for_spawn(dispatch.clone(), BASE + 1)
            .expect("claim"),
    );
    assert!(matches!(
        inbox.mark_process_started_with_failpoint(
            &dispatch.execution_id,
            &claim.claim_id,
            "pid:7:start:11",
            BASE + 2,
            Some(DispatchFailpoint::RunnerStarted),
        ),
        Err(ExecutionDispatchError::SimulatedCrash {
            point: DispatchFailpoint::RunnerStarted
        })
    ));
    drop(inbox);

    let inbox = RunnerExecutionInbox::open(&path, "runner-a").expect("restart");
    assert!(matches!(
        inbox.reconcile().as_slice(),
        [RunnerReconciliationAction::AcceptanceAckRequired { .. }]
    ));
    let entry = inbox.get(&dispatch.execution_id).expect("entry");
    assert_eq!(entry.state, RunnerInboxState::Started);
    assert!(entry.ack.is_none());
}

#[test]
fn owner_restart_reconciles_intent_ack_and_completed_without_transition() {
    let temp = tempfile::tempdir().expect("tempdir");

    let intent_path = temp.path().join("owner-intent.jsonl");
    let intent_dispatch = dispatch("exec-owner-intent");
    let mut owner = OwnerExecutionOutbox::open(&intent_path).expect("open");
    assert!(matches!(
        owner.register_intent_with_failpoint(
            intent_dispatch,
            BASE + 1,
            Some(DispatchFailpoint::OwnerIntent),
        ),
        Err(ExecutionDispatchError::SimulatedCrash {
            point: DispatchFailpoint::OwnerIntent
        })
    ));
    drop(owner);
    let owner = OwnerExecutionOutbox::open(&intent_path).expect("intent restart");
    assert!(matches!(
        owner.reconcile(BASE + 1).as_slice(),
        [OwnerReconciliationAction::RedeliverIntent { .. }]
    ));
    drop(owner);

    let ack_path = temp.path().join("owner-ack.jsonl");
    let ack_dispatch = dispatch("exec-owner-ack");
    let execution_ack = ack(&ack_dispatch);
    let mut owner = OwnerExecutionOutbox::open(&ack_path).expect("open");
    owner
        .register_intent(ack_dispatch, BASE + 1)
        .expect("intent");
    assert!(matches!(
        owner
            .record_ack_with_failpoint(execution_ack, BASE + 3, Some(DispatchFailpoint::OwnerAck),),
        Err(ExecutionDispatchError::SimulatedCrash {
            point: DispatchFailpoint::OwnerAck
        })
    ));
    drop(owner);
    let owner = OwnerExecutionOutbox::open(&ack_path).expect("ACK restart");
    assert!(matches!(
        owner.reconcile(BASE + 3).as_slice(),
        [OwnerReconciliationAction::ApplyExecutingTransition { .. }]
    ));
    drop(owner);

    let result_path = temp.path().join("owner-result.jsonl");
    let result_dispatch = dispatch("exec-owner-result");
    let execution_ack = ack(&result_dispatch);
    let execution_result = result(&result_dispatch);
    let mut owner = OwnerExecutionOutbox::open(&result_path).expect("open");
    owner
        .register_intent(result_dispatch.clone(), BASE + 1)
        .expect("intent");
    owner
        .record_ack(execution_ack.clone(), BASE + 3)
        .expect("ACK");
    owner
        .mark_executing_transition(
            &result_dispatch.execution_id,
            &execution_ack.ack_digest,
            executing_head(),
            BASE + 4,
        )
        .expect("executing");
    assert!(matches!(
        owner.record_result_with_failpoint(
            execution_result,
            BASE + 7,
            Some(DispatchFailpoint::OwnerResult),
        ),
        Err(ExecutionDispatchError::SimulatedCrash {
            point: DispatchFailpoint::OwnerResult
        })
    ));
    drop(owner);
    let owner = OwnerExecutionOutbox::open(&result_path).expect("result restart");
    assert!(matches!(
        owner.reconcile(BASE + 7).as_slice(),
        [OwnerReconciliationAction::ApplyResultTransition { .. }]
    ));
}

#[test]
fn expired_intent_is_typed_and_never_becomes_executing() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("owner-expired.jsonl");
    let dispatch = dispatch("exec-expired");
    let mut owner = OwnerExecutionOutbox::open(&path).expect("open");
    owner
        .register_intent(dispatch.clone(), BASE + 1)
        .expect("intent");
    assert!(matches!(
        owner.reconcile(dispatch.deadline_at).as_slice(),
        [OwnerReconciliationAction::ExpireIntent { .. }]
    ));
    let entry = owner.get(&dispatch.execution_id).expect("entry");
    assert_eq!(entry.state, ExecutionDispatchState::Intent);
    assert!(entry.ack.is_none());
    assert!(entry.executing_head.is_none());
}

#[test]
fn exact_terminal_retries_deduplicate_but_conflicts_refuse() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("owner-dedup.jsonl");
    let dispatch = dispatch("exec-dedup");
    let ack = ack(&dispatch);
    let result = result(&dispatch);
    let mut owner = OwnerExecutionOutbox::open(&path).expect("open");
    owner
        .register_intent(dispatch.clone(), BASE + 1)
        .expect("intent");
    owner.record_ack(ack.clone(), BASE + 3).expect("ACK");
    owner
        .mark_executing_transition(
            &dispatch.execution_id,
            &ack.ack_digest,
            executing_head(),
            BASE + 4,
        )
        .expect("executing");
    owner
        .record_result(result.clone(), BASE + 7)
        .expect("result");
    let before = fs::read(&path).expect("bytes");
    assert_eq!(
        owner
            .record_result(result.clone(), BASE + 8)
            .expect("result retry"),
        DispatchMutation::Deduplicated
    );
    assert_eq!(before, fs::read(&path).expect("bytes"));

    let mut conflict = result;
    conflict.result_id = "different-result".to_string();
    conflict.seal().expect("reseal result");
    assert_eq!(
        owner
            .record_result(conflict, BASE + 8)
            .expect_err("conflicting result")
            .code(),
        "conflicting_execution_result"
    );
}

#[test]
fn hash_chain_torn_tail_swapped_records_and_wrong_surface_fail_closed() {
    let temp = tempfile::tempdir().expect("tempdir");

    let tampered = temp.path().join("tampered.jsonl");
    let mut owner = OwnerExecutionOutbox::open(&tampered).expect("open");
    owner
        .register_intent(dispatch("exec-tamper"), BASE + 1)
        .expect("intent");
    drop(owner);
    let text = String::from_utf8(fs::read(&tampered).expect("read")).expect("UTF-8");
    fs::write(&tampered, text.replacen("brain-a", "brain-z", 1)).expect("tamper");
    assert!(matches!(
        OwnerExecutionOutbox::open(&tampered),
        Err(ExecutionDispatchError::Corruption { .. })
    ));

    let torn = temp.path().join("torn.jsonl");
    fs::write(&torn, b"{\"partial\":true}").expect("write torn");
    assert!(matches!(
        OwnerExecutionOutbox::open(&torn),
        Err(ExecutionDispatchError::Corruption { .. })
    ));

    let swapped = temp.path().join("swapped.jsonl");
    let swap_dispatch = dispatch("exec-swap");
    let mut owner = OwnerExecutionOutbox::open(&swapped).expect("open");
    owner
        .register_intent(swap_dispatch.clone(), BASE + 1)
        .expect("intent");
    owner
        .record_ack(ack(&swap_dispatch), BASE + 3)
        .expect("ACK");
    drop(owner);
    let bytes = fs::read(&swapped).expect("read");
    let mut lines: Vec<&[u8]> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .collect();
    lines.swap(0, 1);
    let mut swapped_bytes = Vec::new();
    for line in lines {
        swapped_bytes.extend_from_slice(line);
        swapped_bytes.push(b'\n');
    }
    fs::write(&swapped, swapped_bytes).expect("write swapped");
    assert!(matches!(
        OwnerExecutionOutbox::open(&swapped),
        Err(ExecutionDispatchError::Corruption { .. })
    ));

    let runner_path = temp.path().join("runner-surface.jsonl");
    let runner = RunnerExecutionInbox::open(&runner_path, "runner-a").expect("open");
    drop(runner);
    let mut runner = RunnerExecutionInbox::open(&runner_path, "runner-a").expect("reopen");
    runner
        .claim_for_spawn(dispatch("exec-surface"), BASE + 1)
        .expect("claim");
    drop(runner);
    assert!(matches!(
        OwnerExecutionOutbox::open(&runner_path),
        Err(ExecutionDispatchError::Corruption { .. })
    ));
}

#[cfg(unix)]
#[test]
fn final_symlink_and_second_writer_are_refused_on_unix() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let target = temp.path().join("target.jsonl");
    fs::write(&target, b"").expect("target");
    let link = temp.path().join("owner-link.jsonl");
    symlink(&target, &link).expect("symlink");
    let symlink_error = match OwnerExecutionOutbox::open(&link) {
        Err(error) => error,
        Ok(_) => panic!("symlink must be refused"),
    };
    assert_eq!(symlink_error.code(), "dispatch_journal_symlink_refused");

    let locked = temp.path().join("locked.jsonl");
    let first = OwnerExecutionOutbox::open(&locked).expect("first writer");
    let lock_error = match OwnerExecutionOutbox::open(&locked) {
        Err(error) => error,
        Ok(_) => panic!("second writer must be refused"),
    };
    assert_eq!(lock_error.code(), "dispatch_journal_writer_lock_refused");
    drop(first);
    OwnerExecutionOutbox::open(&locked).expect("lock released");
}
