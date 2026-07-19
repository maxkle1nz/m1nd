// This contract test compiles the production module directly so private job
// invariants remain testable; unrelated private helpers are expected.
#![allow(dead_code)]

#[path = "../src/runtime_jobs.rs"]
mod runtime_jobs;

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use m1nd_control::{ActionId, AuthorityVariant, Effect, Ingress};
use runtime_jobs::{
    RuntimeJobAuthorityBindingV1, RuntimeJobBindingV1, RuntimeJobError, RuntimeJobFailure,
    RuntimeJobRegistry, RuntimeJobRequestV1, RuntimeJobState, RuntimeJobSuccess, RuntimeJobWait,
    RUNTIME_JOB_AUTHORITY_SCHEMA, RUNTIME_JOB_BINDING_SCHEMA,
};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis()
        .try_into()
        .expect("milliseconds fit u64")
}

fn request(job_id: &str, deadline_after: Duration) -> RuntimeJobRequestV1 {
    RuntimeJobRequestV1 {
        job_id: job_id.to_string(),
        idempotency_key: format!("idem-{job_id}"),
        binding: RuntimeJobBindingV1 {
            schema: RUNTIME_JOB_BINDING_SCHEMA.to_string(),
            organism_id: "organism-test".to_string(),
            brain_id: "brain-test".to_string(),
            mission_id: "mission-test".to_string(),
            agent_id: "agent-test".to_string(),
            action: ActionId::new("graph.ingest").expect("action"),
            ingress: Ingress::BackgroundJob,
            effects: BTreeSet::from([Effect::GraphMutation, Effect::RuntimeStoreWrite]),
            authority: RuntimeJobAuthorityBindingV1 {
                schema: RUNTIME_JOB_AUTHORITY_SCHEMA.to_string(),
                decision_id: "decision-test".to_string(),
                authority_variant: AuthorityVariant::Policy,
                authority_epoch: 9,
                autonomy_epoch: 4,
                capability_id: Some("capability-test".to_string()),
                authorization_digest: "b".repeat(64),
            },
        },
        snapshot_revision: 41,
        deadline_unix_ms: now_ms()
            + u64::try_from(deadline_after.as_millis()).expect("deadline fits u64"),
    }
}

fn terminal(wait: RuntimeJobWait) -> runtime_jobs::RuntimeJobV1 {
    match wait {
        RuntimeJobWait::Terminal(job) => job,
        RuntimeJobWait::ObservableNonTerminal(job) => {
            panic!("expected terminal job, observed {:?}", job.state)
        }
    }
}

#[test]
fn success_is_registered_then_committed_once_and_survives_restart() {
    let temp = tempfile::tempdir().expect("tempdir");
    let journal = temp.path().join("jobs.jsonl");
    let commits = Arc::new(AtomicUsize::new(0));
    let registry = RuntimeJobRegistry::open(&journal).expect("registry");
    let commit_counter = Arc::clone(&commits);
    let job_id = registry
        .submit_prepared(
            request("job-success", Duration::from_secs(2)),
            |context| {
                assert_eq!(context.job_id, "job-success");
                assert_eq!(context.binding.brain_id, "brain-test");
                assert_eq!(context.snapshot_revision, 41);
                assert!(!context.cancellation_token().is_cancelled());
                context.checkpoint()?;
                Ok::<_, RuntimeJobFailure>("prepared-output".to_string())
            },
            move |prepared| {
                assert_eq!(prepared, "prepared-output");
                commit_counter.fetch_add(1, Ordering::SeqCst);
                Ok(RuntimeJobSuccess::new("ok", "commit complete")
                    .with_output_digest("c".repeat(64)))
            },
        )
        .expect("submit");
    let completed = terminal(
        registry
            .wait_terminal(&job_id, Duration::from_secs(2))
            .expect("wait"),
    );
    assert_eq!(completed.state, RuntimeJobState::Succeeded);
    assert_eq!(commits.load(Ordering::SeqCst), 1);
    assert_eq!(completed.binding.brain_id, "brain-test");
    assert_eq!(completed.snapshot_revision, 41);
    assert_eq!(registry.journal_path().expect("journal path"), journal);
    assert_eq!(registry.list().expect("jobs").len(), 1);
    let health = registry.health_snapshot().expect("health");
    assert_eq!(health.active_jobs, 0);
    assert_eq!(health.total_jobs, 1);
    drop(registry);

    let reopened = RuntimeJobRegistry::open(&journal).expect("reopen");
    let replayed = reopened.get(&job_id).expect("replayed job");
    assert_eq!(replayed.state, RuntimeJobState::Succeeded);
    assert_eq!(
        replayed
            .terminal_result
            .as_ref()
            .and_then(|result| result.output_digest.as_deref()),
        Some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
    );
    assert_eq!(commits.load(Ordering::SeqCst), 1);
}

#[cfg(unix)]
#[test]
fn second_writer_for_same_journal_is_refused() {
    let temp = tempfile::tempdir().expect("tempdir");
    let journal = temp.path().join("jobs.jsonl");
    let first = RuntimeJobRegistry::open(&journal).expect("first writer");
    let second = RuntimeJobRegistry::open(&journal);
    assert!(matches!(
        second,
        Err(RuntimeJobError::Refused {
            code: "journal_writer_lock_refused",
            ..
        })
    ));
    drop(first);
    RuntimeJobRegistry::open(&journal).expect("lock released on drop");
}

#[test]
fn timeout_is_observable_and_late_prepared_write_never_commits() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = RuntimeJobRegistry::open(temp.path().join("jobs.jsonl")).expect("registry");
    let commits = Arc::new(AtomicUsize::new(0));
    let commit_counter = Arc::clone(&commits);
    let job_id = registry
        .submit_prepared(
            request("job-timeout", Duration::from_millis(35)),
            |_context| {
                thread::sleep(Duration::from_millis(180));
                Ok::<_, RuntimeJobFailure>("late-prepared-write")
            },
            move |_prepared| {
                commit_counter.fetch_add(1, Ordering::SeqCst);
                Ok(RuntimeJobSuccess::new("unexpected", "must not commit"))
            },
        )
        .expect("submit");

    thread::sleep(Duration::from_millis(65));
    let during_timeout = registry.get(&job_id).expect("observable job");
    assert_eq!(during_timeout.state, RuntimeJobState::Cancelling);
    assert!(during_timeout.running_after_timeout);
    assert_eq!(commits.load(Ordering::SeqCst), 0);

    let completed = terminal(
        registry
            .wait_terminal(&job_id, Duration::from_secs(2))
            .expect("wait"),
    );
    assert_eq!(completed.state, RuntimeJobState::Cancelled);
    assert!(completed.running_after_timeout);
    assert_eq!(commits.load(Ordering::SeqCst), 0);
}

#[test]
fn explicit_cancellation_wins_before_preparation_completion() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = RuntimeJobRegistry::open(temp.path().join("jobs.jsonl")).expect("registry");
    let started = Arc::new(AtomicBool::new(false));
    let commits = Arc::new(AtomicUsize::new(0));
    let started_worker = Arc::clone(&started);
    let commit_counter = Arc::clone(&commits);
    let job_id = registry
        .submit_prepared(
            request("job-cancel-first", Duration::from_secs(2)),
            move |context| {
                started_worker.store(true, Ordering::Release);
                while !context.is_cancelled() {
                    thread::yield_now();
                }
                Ok::<_, RuntimeJobFailure>("prepared-after-cancel")
            },
            move |_prepared| {
                commit_counter.fetch_add(1, Ordering::SeqCst);
                Ok(RuntimeJobSuccess::new("unexpected", "must not commit"))
            },
        )
        .expect("submit");
    while !started.load(Ordering::Acquire) {
        thread::yield_now();
    }
    registry.request_cancel(&job_id).expect("cancel");
    let completed = terminal(
        registry
            .wait_terminal(&job_id, Duration::from_secs(2))
            .expect("wait"),
    );
    assert_eq!(completed.state, RuntimeJobState::Cancelled);
    assert!(!completed.running_after_timeout);
    assert_eq!(commits.load(Ordering::SeqCst), 0);
}

#[test]
fn completion_linearizes_before_concurrent_cancel() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = RuntimeJobRegistry::open(temp.path().join("jobs.jsonl")).expect("registry");
    let commit_entered = Arc::new(Barrier::new(2));
    let release_commit = Arc::new(Barrier::new(2));
    let entered = Arc::clone(&commit_entered);
    let release = Arc::clone(&release_commit);
    let job_id = registry
        .submit_prepared(
            request("job-complete-first", Duration::from_secs(2)),
            |_context| Ok::<_, RuntimeJobFailure>("prepared"),
            move |_prepared| {
                entered.wait();
                release.wait();
                Ok(RuntimeJobSuccess::new("ok", "completion won"))
            },
        )
        .expect("submit");
    commit_entered.wait();

    let health_started = Instant::now();
    let health = registry.health_snapshot().expect("health during commit");
    assert!(
        health_started.elapsed() < Duration::from_millis(100),
        "health must not wait for the commit closure"
    );
    assert_eq!(health.commit_in_progress, 1);
    assert_eq!(health.active_jobs, 1);
    assert!(registry.get(&job_id).expect("job").commit_in_progress);

    let cancel_started = Instant::now();
    let cancel = registry.request_cancel(&job_id);
    assert!(
        cancel_started.elapsed() < Duration::from_millis(100),
        "cancel observation must not wait for the commit closure"
    );
    assert!(matches!(
        cancel,
        Err(RuntimeJobError::CancellationTooLate(ref observed)) if observed == &job_id
    ));
    release_commit.wait();
    let completed = terminal(
        registry
            .wait_terminal(&job_id, Duration::from_secs(2))
            .expect("wait"),
    );
    assert_eq!(completed.state, RuntimeJobState::Succeeded);
    assert!(!completed.commit_in_progress);
}

#[test]
fn reserved_commit_crossing_deadline_stays_observable_and_is_not_false_cancelled() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = RuntimeJobRegistry::open(temp.path().join("jobs.jsonl")).expect("registry");
    let commit_entered = Arc::new(Barrier::new(2));
    let release_commit = Arc::new(Barrier::new(2));
    let entered = Arc::clone(&commit_entered);
    let release = Arc::clone(&release_commit);
    let job_id = registry
        .submit_prepared(
            request("job-commit-over-deadline", Duration::from_millis(300)),
            |_context| Ok::<_, RuntimeJobFailure>(()),
            move |_| {
                entered.wait();
                release.wait();
                Ok(RuntimeJobSuccess::new("ok", "reserved commit completed"))
            },
        )
        .expect("submit");
    commit_entered.wait();
    thread::sleep(Duration::from_millis(350));

    let observable = registry.get(&job_id).expect("observable job");
    assert_eq!(observable.state, RuntimeJobState::Running);
    assert!(observable.commit_in_progress);
    assert!(observable.running_after_timeout);
    let health = registry.health_snapshot().expect("health");
    assert_eq!(health.running_after_timeout, 1);
    assert!(matches!(
        registry.request_cancel(&job_id),
        Err(RuntimeJobError::CancellationTooLate(_))
    ));

    release_commit.wait();
    let completed = terminal(
        registry
            .wait_terminal(&job_id, Duration::from_secs(2))
            .expect("wait"),
    );
    assert_eq!(completed.state, RuntimeJobState::Succeeded);
    assert!(completed.running_after_timeout);
    assert_eq!(
        completed.state_reason.as_deref(),
        Some("commit_completed_after_deadline")
    );
}

#[test]
fn duplicate_job_and_idempotency_collisions_are_refused() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = RuntimeJobRegistry::open(temp.path().join("jobs.jsonl")).expect("registry");
    let blocker = Arc::new(Barrier::new(2));
    let worker_blocker = Arc::clone(&blocker);
    registry
        .submit_prepared(
            request("job-duplicate", Duration::from_secs(2)),
            move |_context| {
                worker_blocker.wait();
                Ok::<_, RuntimeJobFailure>(())
            },
            |_| Ok(RuntimeJobSuccess::new("ok", "done")),
        )
        .expect("first submit");

    let duplicate = registry.submit_prepared(
        request("job-duplicate", Duration::from_secs(2)),
        |_context| Ok::<_, RuntimeJobFailure>(()),
        |_| Ok(RuntimeJobSuccess::new("ok", "done")),
    );
    assert!(matches!(duplicate, Err(RuntimeJobError::DuplicateJobId(_))));

    let mut idempotency_collision = request("job-other", Duration::from_secs(2));
    idempotency_collision.idempotency_key = "idem-job-duplicate".to_string();
    let collision = registry.submit_prepared(
        idempotency_collision,
        |_context| Ok::<_, RuntimeJobFailure>(()),
        |_| Ok(RuntimeJobSuccess::new("ok", "done")),
    );
    assert!(matches!(
        collision,
        Err(RuntimeJobError::DuplicateIdempotencyKey(_))
    ));
    blocker.wait();
    let _ = registry
        .wait_terminal("job-duplicate", Duration::from_secs(2))
        .expect("finish original");
}

#[test]
fn bounded_capacity_refuses_overload_and_exposes_health() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = RuntimeJobRegistry::open_with_max_in_flight(temp.path().join("jobs.jsonl"), 1)
        .expect("registry");
    let blocker = Arc::new(Barrier::new(2));
    let worker_blocker = Arc::clone(&blocker);
    registry
        .submit_prepared(
            request("job-capacity-a", Duration::from_secs(2)),
            move |_context| {
                worker_blocker.wait();
                Ok::<_, RuntimeJobFailure>(())
            },
            |_| Ok(RuntimeJobSuccess::new("ok", "done")),
        )
        .expect("first submit");

    let overloaded = registry.submit_prepared(
        request("job-capacity-b", Duration::from_secs(2)),
        |_context| Ok::<_, RuntimeJobFailure>(()),
        |_| Ok(RuntimeJobSuccess::new("ok", "done")),
    );
    assert!(matches!(
        overloaded,
        Err(RuntimeJobError::Overloaded {
            limit: 1,
            active: 1
        })
    ));
    let health = registry.health_snapshot().expect("health");
    assert_eq!(health.max_in_flight, 1);
    assert_eq!(health.active_jobs, 1);
    assert_eq!(health.state_counts.get("RUNNING"), Some(&1));
    blocker.wait();
    let _ = registry
        .wait_terminal("job-capacity-a", Duration::from_secs(2))
        .expect("finish");
}

#[test]
fn commit_panic_is_failed_terminal_not_a_poisoned_registry() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = RuntimeJobRegistry::open(temp.path().join("jobs.jsonl")).expect("registry");
    registry
        .submit_prepared(
            request("job-commit-panic", Duration::from_secs(2)),
            |_context| Ok::<_, RuntimeJobFailure>(()),
            |_| -> Result<RuntimeJobSuccess, RuntimeJobFailure> {
                panic!("simulated commit panic")
            },
        )
        .expect("submit");
    let failed = terminal(
        registry
            .wait_terminal("job-commit-panic", Duration::from_secs(2))
            .expect("wait"),
    );
    assert_eq!(failed.state, RuntimeJobState::Failed);
    assert_eq!(
        failed
            .terminal_result
            .as_ref()
            .map(|result| result.code.as_str()),
        Some("commit_panicked")
    );
    assert!(!registry.health_snapshot().expect("health").poisoned);
}

#[test]
fn shutdown_drains_cooperative_work_and_refuses_new_jobs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = RuntimeJobRegistry::open(temp.path().join("jobs.jsonl")).expect("registry");
    let started = Arc::new(AtomicBool::new(false));
    let worker_started = Arc::clone(&started);
    registry
        .submit_prepared(
            request("job-shutdown", Duration::from_secs(2)),
            move |context| {
                worker_started.store(true, Ordering::Release);
                while !context.is_cancelled() {
                    thread::yield_now();
                }
                Ok::<_, RuntimeJobFailure>(())
            },
            |_| Ok(RuntimeJobSuccess::new("unexpected", "must not commit")),
        )
        .expect("submit");
    while !started.load(Ordering::Acquire) {
        thread::yield_now();
    }
    registry
        .shutdown(Duration::from_secs(2))
        .expect("cooperative shutdown");
    assert_eq!(
        registry.get("job-shutdown").expect("job").state,
        RuntimeJobState::Cancelled
    );
    let refused = registry.submit_prepared(
        request("job-after-shutdown", Duration::from_secs(1)),
        |_context| Ok::<_, RuntimeJobFailure>(()),
        |_| Ok(RuntimeJobSuccess::new("ok", "done")),
    );
    assert!(matches!(
        refused,
        Err(RuntimeJobError::RegistryShuttingDown)
    ));
}

#[test]
fn shutdown_timeout_fails_closed_with_observable_active_job() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = RuntimeJobRegistry::open(temp.path().join("jobs.jsonl")).expect("registry");
    let commits = Arc::new(AtomicUsize::new(0));
    let commit_counter = Arc::clone(&commits);
    registry
        .submit_prepared(
            request("job-slow-shutdown", Duration::from_secs(2)),
            |_context| {
                thread::sleep(Duration::from_millis(180));
                Ok::<_, RuntimeJobFailure>(())
            },
            move |_| {
                commit_counter.fetch_add(1, Ordering::SeqCst);
                Ok(RuntimeJobSuccess::new("unexpected", "must not commit"))
            },
        )
        .expect("submit");
    let shutdown = registry.shutdown(Duration::from_millis(20));
    assert!(matches!(
        shutdown,
        Err(RuntimeJobError::ShutdownIncomplete(_))
    ));
    assert_eq!(
        registry.get("job-slow-shutdown").expect("job").state,
        RuntimeJobState::Cancelling
    );
    let completed = terminal(
        registry
            .wait_terminal("job-slow-shutdown", Duration::from_secs(2))
            .expect("wait"),
    );
    assert_eq!(completed.state, RuntimeJobState::Cancelled);
    assert_eq!(commits.load(Ordering::SeqCst), 0);
}
