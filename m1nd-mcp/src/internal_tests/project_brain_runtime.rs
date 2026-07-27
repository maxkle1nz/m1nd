use crate as m1nd_mcp;

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use m1nd_control::{ActionId, AuthorityVariant, Effect, Ingress};
use m1nd_mcp::brain_runtime::BRAIN_CHECKPOINT_DIRECTORY;
use m1nd_mcp::checkpoint_store::{CheckpointLoadDisposition, GRAPH_SNAPSHOT_LOGICAL_NAME};
use m1nd_mcp::project_brains::ProjectBrainRegistry;
use m1nd_mcp::runtime_jobs::{
    RuntimeJobAuthorityBindingV1, RuntimeJobBindingV1, RuntimeJobFailure, RuntimeJobRequestV1,
    RuntimeJobState, RuntimeJobSuccess, RuntimeJobV1, RuntimeJobWait, RUNTIME_JOB_AUTHORITY_SCHEMA,
    RUNTIME_JOB_BINDING_SCHEMA,
};

/// Upper bound for awaiting a runtime job's TERMINAL state. `wait_terminal` is
/// condvar-based and returns the instant the job finishes, so this only caps a
/// stuck run: a healthy run pays nothing for the headroom. Generous on purpose —
/// on the loaded two-core Windows runner the m1nd-mcp lib suite measures ~1001s
/// against ~440-700s on a developer Mac, so a five-second wait was measuring the
/// runner rather than the job.
const TERMINAL_BUDGET: Duration = Duration::from_secs(60);

/// Grace handed to `ProjectBrainRegistry::shutdown`. The product contract is
/// "fail closed unless every actor checkpoints within the grace the CALLER gave";
/// this constant is only the test's tolerance for a slow machine. Shutdown returns
/// the instant the last ACK lands, so the headroom costs nothing when healthy.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(60);

/// Upper bound for waiting on a condition (a refusal arriving, a health status
/// flipping) rather than timing a single look. Bounds a stuck run only.
const OBSERVE_BUDGET: Duration = Duration::from_secs(30);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis()
        .try_into()
        .expect("milliseconds fit")
}

fn mount(registry: &ProjectBrainRegistry, root: &std::path::Path) {
    std::fs::create_dir_all(root).expect("project root");
    registry
        .ensure_registered(&root.to_string_lossy())
        .expect("register project brain");
    registry
        .try_resolve(&root.to_string_lossy())
        .expect("resolve")
        .expect("warm brain");
}

fn request(
    registry: &ProjectBrainRegistry,
    root: &std::path::Path,
    job_id: &str,
    snapshot_revision: u64,
) -> RuntimeJobRequestV1 {
    RuntimeJobRequestV1 {
        job_id: job_id.to_string(),
        idempotency_key: format!("idem-{job_id}"),
        binding: RuntimeJobBindingV1 {
            schema: RUNTIME_JOB_BINDING_SCHEMA.to_string(),
            organism_id: "organism-runtime-test".to_string(),
            brain_id: registry.brain_id_for(&root.to_string_lossy()),
            mission_id: "mission-runtime-test".to_string(),
            agent_id: "agent-runtime-test".to_string(),
            action: ActionId::new("graph.background-mutation").expect("action"),
            ingress: Ingress::BackgroundJob,
            effects: BTreeSet::from([Effect::GraphMutation, Effect::RuntimeStoreWrite]),
            authority: RuntimeJobAuthorityBindingV1 {
                schema: RUNTIME_JOB_AUTHORITY_SCHEMA.to_string(),
                decision_id: format!("decision-{job_id}"),
                authority_variant: AuthorityVariant::Policy,
                authority_epoch: 1,
                autonomy_epoch: 0,
                capability_id: None,
                authorization_digest: "a".repeat(64),
            },
        },
        snapshot_revision,
        deadline_unix_ms: now_ms() + 10_000,
    }
}

fn current_revision(registry: &ProjectBrainRegistry, root: &std::path::Path) -> u64 {
    registry
        .read_runtime_snapshot(&root.to_string_lossy(), |_state| {
            Ok::<_, RuntimeJobFailure>(())
        })
        .expect("snapshot")
        .version
        .revision
}

fn terminal(wait: RuntimeJobWait) -> RuntimeJobV1 {
    match wait {
        RuntimeJobWait::Terminal(job) => job,
        RuntimeJobWait::ObservableNonTerminal(job) => {
            panic!("expected terminal job, got {:?}", job.state)
        }
    }
}

fn submit_root_append(
    registry: &ProjectBrainRegistry,
    root: &std::path::Path,
    job_id: &str,
    marker: impl Into<String>,
) -> String {
    let marker = marker.into();
    let revision = current_revision(registry, root);
    registry
        .submit_runtime_job(
            &root.to_string_lossy(),
            request(registry, root, job_id, revision),
            |state| Ok::<_, RuntimeJobFailure>(state.ingest_roots.clone()),
            move |context, snapshot| {
                context.checkpoint()?;
                assert_eq!(context.snapshot_revision, snapshot.version.revision);
                Ok::<_, RuntimeJobFailure>(marker)
            },
            |state, proposal| {
                state.ingest_roots.push(proposal);
                state.bump_graph_generation();
                Ok(RuntimeJobSuccess::new("committed", "actor commit complete"))
            },
        )
        .expect("submit")
}

fn submit_stress_append(
    registry: &ProjectBrainRegistry,
    root: &std::path::Path,
    job_id: &str,
    snapshot_revision: u64,
    marker: String,
) -> Result<String, String> {
    registry
        .submit_runtime_job(
            &root.to_string_lossy(),
            request(registry, root, job_id, snapshot_revision),
            |state| Ok::<_, RuntimeJobFailure>(state.ingest_roots.clone()),
            move |context, snapshot| {
                context.checkpoint()?;
                if context.snapshot_revision != snapshot.version.revision {
                    return Err(RuntimeJobFailure::new(
                        "stress_snapshot_mismatch",
                        "prepared snapshot revision changed before commit",
                    ));
                }
                Ok::<_, RuntimeJobFailure>(marker)
            },
            |state, proposal| {
                state.ingest_roots.push(proposal);
                state.bump_graph_generation();
                Ok(RuntimeJobSuccess::new(
                    "stress_committed",
                    "stress actor commit complete",
                ))
            },
        )
        .map_err(|error| format!("submit stress job {job_id}: {error}"))
}

#[test]
fn stale_concurrent_worker_never_mutates_and_only_one_write_commits() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("repo");
    let registry = ProjectBrainRegistry::with_capacity(temp.path().join("brains"), None, 2)
        .with_runtime_limits(4, 2);
    mount(&registry, &root);

    let revision = current_revision(&registry, &root);
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let stale_apply_calls = Arc::new(AtomicUsize::new(0));
    let stale_apply_counter = stale_apply_calls.clone();
    let slow_id = registry
        .submit_runtime_job(
            &root.to_string_lossy(),
            request(&registry, &root, "slow-stale", revision),
            |_state| Ok::<_, RuntimeJobFailure>("snapshot".to_string()),
            move |_context, _snapshot| {
                started_tx.send(()).expect("signal slow worker");
                release_rx.recv().expect("release slow worker");
                Ok::<_, RuntimeJobFailure>("stale-marker".to_string())
            },
            move |state, proposal| {
                stale_apply_counter.fetch_add(1, Ordering::SeqCst);
                state.ingest_roots.push(proposal);
                state.bump_graph_generation();
                Ok(RuntimeJobSuccess::new("unexpected", "stale commit ran"))
            },
        )
        .expect("submit slow");
    started_rx.recv().expect("slow worker started");

    let fast_id = submit_root_append(&registry, &root, "fast-winner", "winner-marker");
    let jobs = registry.runtime_job_registry().expect("job registry");
    let fast = terminal(
        jobs.wait_terminal(&fast_id, TERMINAL_BUDGET)
            .expect("wait fast"),
    );
    assert_eq!(fast.state, RuntimeJobState::Succeeded);

    release_tx.send(()).expect("release stale worker");
    let slow = terminal(
        jobs.wait_terminal(&slow_id, TERMINAL_BUDGET)
            .expect("wait slow"),
    );
    assert_eq!(slow.state, RuntimeJobState::Failed);
    assert_eq!(
        slow.terminal_result
            .as_ref()
            .map(|result| result.code.as_str()),
        Some("brain_snapshot_stale")
    );
    assert_eq!(stale_apply_calls.load(Ordering::SeqCst), 0);
    let roots = registry
        .read_runtime_snapshot(&root.to_string_lossy(), |state| {
            Ok::<_, RuntimeJobFailure>(state.ingest_roots.clone())
        })
        .expect("read roots")
        .value;
    assert!(roots.iter().any(|root| root == "winner-marker"));
    assert!(!roots.iter().any(|root| root == "stale-marker"));
    registry.shutdown(SHUTDOWN_GRACE).expect("shutdown");
}

#[test]
fn multi_brain_actors_are_isolated() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root_a = temp.path().join("repo-a");
    let root_b = temp.path().join("repo-b");
    let registry = Arc::new(ProjectBrainRegistry::with_capacity(
        temp.path().join("brains"),
        None,
        4,
    ));
    mount(&registry, &root_a);
    mount(&registry, &root_b);

    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let blocked_registry = registry.clone();
    let blocked_root = root_a.clone();
    let blocked_entered = entered.clone();
    let blocked_release = release.clone();
    let blocked_a = thread::spawn(move || {
        blocked_registry.read_runtime_snapshot(&blocked_root.to_string_lossy(), move |_state| {
            blocked_entered.wait();
            blocked_release.wait();
            Ok::<_, RuntimeJobFailure>(())
        })
    });
    entered.wait();
    let b_read_started = Instant::now();
    registry
        .read_runtime_snapshot(&root_b.to_string_lossy(), |_state| {
            Ok::<_, RuntimeJobFailure>(())
        })
        .expect("brain B remains readable while A actor is blocked");
    // Isolation is proved by the STRUCTURE, not by this clock: A is parked
    // inside its callback and is only released further down, on this very
    // thread, so a B read that serialized behind A would deadlock here rather
    // than merely run late. The bound is kept as a coarse "did not hang" net
    // and is deliberately generous — at one second it was measuring how loaded
    // the CI runner was, and failed on Windows for exactly that reason.
    assert!(b_read_started.elapsed() < Duration::from_secs(60));
    release.wait();
    blocked_a
        .join()
        .expect("blocked A thread")
        .expect("blocked A read");

    let job_a = submit_root_append(&registry, &root_a, "brain-a-job", "only-a");
    let jobs = registry.runtime_job_registry().expect("jobs");
    assert_eq!(
        terminal(jobs.wait_terminal(&job_a, TERMINAL_BUDGET).expect("wait A")).state,
        RuntimeJobState::Succeeded
    );
    let roots_a = registry
        .read_runtime_snapshot(&root_a.to_string_lossy(), |state| {
            Ok::<_, RuntimeJobFailure>(state.ingest_roots.clone())
        })
        .expect("read A")
        .value;
    let roots_b = registry
        .read_runtime_snapshot(&root_b.to_string_lossy(), |state| {
            Ok::<_, RuntimeJobFailure>(state.ingest_roots.clone())
        })
        .expect("read B")
        .value;
    assert!(roots_a.iter().any(|root| root == "only-a"));
    assert!(!roots_b.iter().any(|root| root == "only-a"));
    assert_ne!(
        registry.brain_id_for(&root_a.to_string_lossy()),
        registry.brain_id_for(&root_b.to_string_lossy())
    );
    let acks = registry.shutdown(SHUTDOWN_GRACE).expect("shutdown");
    assert_eq!(acks.len(), 2);
    assert_ne!(acks[0].brain_id, acks[1].brain_id);
}

#[test]
fn ten_thousand_multi_project_actor_operations_are_isolated_and_bounded() {
    const BRAIN_COUNT: usize = 8;
    const OPERATIONS_PER_BRAIN: usize = 1_250;
    const WRITE_EVERY: usize = 125;
    const TOTAL_OPERATIONS: usize = BRAIN_COUNT * OPERATIONS_PER_BRAIN;
    const WRITES_PER_BRAIN: usize = OPERATIONS_PER_BRAIN / WRITE_EVERY;
    const TOTAL_WRITES: usize = BRAIN_COUNT * WRITES_PER_BRAIN;
    const TOTAL_READS: usize = TOTAL_OPERATIONS - TOTAL_WRITES;
    // A LIVELOCK / LOST-WORKER net, not a throughput SLO: nothing in the product
    // promises 10,000 actor operations inside any wall-clock window, and the proof
    // this test carries is isolation plus the exact read/write ledger below. At
    // sixty seconds the bound was measuring the runner instead — eight worker
    // threads doing real OCC submissions on a loaded two-core box (the m1nd-mcp lib
    // suite measures ~1001s there against ~440-700s on a developer Mac) blew it and
    // reported "exceeded 60s or lost a worker". Five minutes still catches a real
    // hang in minutes, and a healthy run neither waits for it nor hides behind it:
    // the true elapsed time is asserted-on-nothing but PRINTED below, so a genuine
    // throughput regression stays visible in the log.
    const COMPLETION_BOUND: Duration = Duration::from_secs(300);

    let temp = tempfile::tempdir().expect("tempdir");
    let registry = Arc::new(
        ProjectBrainRegistry::with_capacity(temp.path().join("brains"), None, BRAIN_COUNT)
            .with_runtime_limits(BRAIN_COUNT, 2_048),
    );
    let roots = (0..BRAIN_COUNT)
        .map(|brain_index| temp.path().join(format!("repo-{brain_index:02}")))
        .collect::<Vec<_>>();
    for root in &roots {
        mount(&registry, root);
    }
    // Setup probes and final verification are deliberately outside the measured
    // ledger. The concurrent window below contains exactly 10,000 public
    // registry operations: 9,920 reads plus 80 OCC job submissions.
    let initial_revisions = roots
        .iter()
        .map(|root| current_revision(&registry, root))
        .collect::<Vec<_>>();

    let start = Arc::new(Barrier::new(BRAIN_COUNT + 1));
    let completed = Arc::new(AtomicUsize::new(0));
    let reads = Arc::new(AtomicUsize::new(0));
    let writes = Arc::new(AtomicUsize::new(0));
    let (result_tx, result_rx) = mpsc::channel::<Result<(usize, usize, usize), String>>();
    let mut workers = Vec::with_capacity(BRAIN_COUNT);

    for (brain_index, (root, initial_revision)) in
        roots.iter().cloned().zip(initial_revisions).enumerate()
    {
        let registry = registry.clone();
        let start = start.clone();
        let completed = completed.clone();
        let reads = reads.clone();
        let writes = writes.clone();
        let result_tx = result_tx.clone();
        workers.push(thread::spawn(move || {
            let own_prefix = format!("r6-stress-brain-{brain_index:02}/");
            start.wait();
            let result = (|| -> Result<(usize, usize, usize), String> {
                let mut local_reads = 0usize;
                let mut local_writes = 0usize;
                let mut next_revision = initial_revision;
                for operation in 0..OPERATIONS_PER_BRAIN {
                    if operation % WRITE_EVERY == 0 {
                        let marker = format!("{own_prefix}write-{local_writes:02}");
                        let job_id = format!("r6-stress-{brain_index:02}-{local_writes:02}");
                        let job_id = submit_stress_append(
                            &registry,
                            &root,
                            &job_id,
                            next_revision,
                            marker,
                        )?;
                        let job = match registry
                            .runtime_job_registry()
                            .map_err(|error| format!("open stress job registry: {error}"))?
                            .wait_terminal(&job_id, TERMINAL_BUDGET)
                            .map_err(|error| format!("wait stress job {job_id}: {error}"))?
                        {
                            RuntimeJobWait::Terminal(job) => job,
                            RuntimeJobWait::ObservableNonTerminal(job) => {
                                return Err(format!(
                                    "stress job {job_id} did not terminate before its {TERMINAL_BUDGET:?} wait: {:?}",
                                    job.state
                                ));
                            }
                        };
                        if job.state != RuntimeJobState::Succeeded {
                            return Err(format!(
                                "stress job {job_id} terminated as {:?}: {:?}",
                                job.state, job.terminal_result
                            ));
                        }
                        next_revision = next_revision.saturating_add(1);
                        local_writes += 1;
                        writes.fetch_add(1, Ordering::SeqCst);
                    } else {
                        let stress_markers = registry
                            .read_runtime_snapshot(&root.to_string_lossy(), |state| {
                                Ok::<_, RuntimeJobFailure>(
                                    state
                                        .ingest_roots
                                        .iter()
                                        .filter(|value| value.starts_with("r6-stress-brain-"))
                                        .cloned()
                                        .collect::<Vec<_>>(),
                                )
                            })
                            .map_err(|error| {
                                format!(
                                    "brain {brain_index:02} read {local_reads} failed: {error}"
                                )
                            })?
                            .value;
                        if let Some(foreign) = stress_markers
                            .iter()
                            .find(|marker| !marker.starts_with(&own_prefix))
                        {
                            return Err(format!(
                                "cross-brain leakage into brain {brain_index:02}: {foreign}"
                            ));
                        }
                        local_reads += 1;
                        reads.fetch_add(1, Ordering::SeqCst);
                    }
                    completed.fetch_add(1, Ordering::SeqCst);
                }
                Ok((brain_index, local_reads, local_writes))
            })();
            let _ = result_tx.send(result);
        }));
    }
    drop(result_tx);

    let started = Instant::now();
    let deadline = started + COMPLETION_BOUND;
    start.wait();
    let mut worker_results = Vec::with_capacity(BRAIN_COUNT);
    for _ in 0..BRAIN_COUNT {
        let remaining = deadline.saturating_duration_since(Instant::now());
        worker_results.push(result_rx.recv_timeout(remaining).unwrap_or_else(|error| {
            panic!(
                "10,000-operation stress hung past {COMPLETION_BOUND:?} or lost a worker: {error}"
            )
        }));
    }
    for worker in workers {
        worker.join().expect("stress worker panicked");
    }
    for result in worker_results {
        let (brain_index, local_reads, local_writes) = result.unwrap_or_else(|error| {
            panic!("stress worker failed: {error}");
        });
        assert_eq!(local_reads, OPERATIONS_PER_BRAIN - WRITES_PER_BRAIN);
        assert_eq!(local_writes, WRITES_PER_BRAIN);
        assert!(brain_index < BRAIN_COUNT);
    }

    let stress_elapsed = started.elapsed();
    assert!(
        stress_elapsed < COMPLETION_BOUND,
        "10,000 logical actor operations hung past {COMPLETION_BOUND:?}"
    );
    assert_eq!(completed.load(Ordering::SeqCst), TOTAL_OPERATIONS);
    assert_eq!(reads.load(Ordering::SeqCst), TOTAL_READS);
    assert_eq!(writes.load(Ordering::SeqCst), TOTAL_WRITES);

    for (brain_index, root) in roots.iter().enumerate() {
        let own_prefix = format!("r6-stress-brain-{brain_index:02}/");
        let observed = registry
            .read_runtime_snapshot(&root.to_string_lossy(), |state| {
                Ok::<_, RuntimeJobFailure>(
                    state
                        .ingest_roots
                        .iter()
                        .filter(|value| value.starts_with("r6-stress-brain-"))
                        .cloned()
                        .collect::<BTreeSet<_>>(),
                )
            })
            .expect("final isolation snapshot")
            .value;
        let expected = (0..WRITES_PER_BRAIN)
            .map(|write_index| format!("{own_prefix}write-{write_index:02}"))
            .collect::<BTreeSet<_>>();
        assert_eq!(observed, expected, "brain {brain_index:02} marker set");
    }

    let acks = registry
        .shutdown(SHUTDOWN_GRACE)
        .expect("bounded stress shutdown");
    assert_eq!(acks.len(), BRAIN_COUNT);
    assert_eq!(
        acks.iter()
            .map(|ack| ack.brain_id.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        BRAIN_COUNT
    );
    eprintln!(
        "R6 multi-brain stress proof: brains={BRAIN_COUNT}, operations={TOTAL_OPERATIONS}, reads={TOTAL_READS}, writes={TOTAL_WRITES}, elapsed={stress_elapsed:?}, completion_bound={COMPLETION_BOUND:?}"
    );
}

#[test]
fn actor_queue_and_global_worker_limit_fail_fast_under_pressure() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("repo");
    let registry = Arc::new(
        ProjectBrainRegistry::with_capacity(temp.path().join("brains"), None, 2)
            .with_runtime_limits(1, 1),
    );
    mount(&registry, &root);
    current_revision(&registry, &root); // start actor

    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let blocking_registry = registry.clone();
    let blocking_root = root.clone();
    let entered_worker = entered.clone();
    let release_worker = release.clone();
    let blocker = thread::spawn(move || {
        blocking_registry.read_runtime_snapshot(&blocking_root.to_string_lossy(), move |_state| {
            entered_worker.wait();
            release_worker.wait();
            Ok::<_, RuntimeJobFailure>(())
        })
    });
    entered.wait();

    let (result_tx, result_rx) = mpsc::channel();
    let mut contenders = Vec::new();
    for _ in 0..2 {
        let contender_registry = registry.clone();
        let contender_root = root.clone();
        let tx = result_tx.clone();
        contenders.push(thread::spawn(move || {
            let result = contender_registry
                .read_runtime_snapshot(&contender_root.to_string_lossy(), |_state| {
                    Ok::<_, RuntimeJobFailure>(())
                })
                .map(|_| ())
                .map_err(|error| error.to_string());
            tx.send(result).expect("send contender result");
        }));
    }
    let refused = result_rx
        .recv_timeout(OBSERVE_BUDGET)
        .expect("one contender must fail fast while actor+queue are occupied");
    assert!(refused
        .expect_err("one contender must be refused")
        .contains("brain_actor_queue_full"));
    release.wait();
    blocker
        .join()
        .expect("blocker thread")
        .expect("blocker read");
    let accepted = result_rx
        .recv_timeout(OBSERVE_BUDGET)
        .expect("queued contender completes");
    assert!(accepted.is_ok());
    for contender in contenders {
        contender.join().expect("contender thread");
    }

    let revision = current_revision(&registry, &root);
    let (prepare_started_tx, prepare_started_rx) = mpsc::sync_channel(1);
    let (prepare_release_tx, prepare_release_rx) = mpsc::sync_channel(1);
    let first_job = registry
        .submit_runtime_job(
            &root.to_string_lossy(),
            request(&registry, &root, "worker-slot-owner", revision),
            |_state| Ok::<_, RuntimeJobFailure>(()),
            move |_context, _snapshot| {
                prepare_started_tx.send(()).expect("worker started");
                prepare_release_rx.recv().expect("worker released");
                Ok::<_, RuntimeJobFailure>(())
            },
            |state, ()| {
                state.bump_graph_generation();
                Ok(RuntimeJobSuccess::new("ok", "slot owner committed"))
            },
        )
        .expect("first job");
    prepare_started_rx.recv().expect("first prepare started");
    let overload_started = Instant::now();
    let overload = registry.submit_runtime_job(
        &root.to_string_lossy(),
        request(&registry, &root, "worker-overload", revision),
        |_state| Ok::<_, RuntimeJobFailure>(()),
        |_context, _snapshot| Ok::<_, RuntimeJobFailure>(()),
        |_state, ()| Ok(RuntimeJobSuccess::new("unexpected", "must not commit")),
    );
    assert!(overload.is_err());
    // Fail-fast is proved by the STRUCTURE, not by this clock: the only worker slot
    // is parked inside `prepare` waiting on `prepare_release_tx`, which is only sent
    // further down on THIS thread — so an overload submission that queued for a slot
    // instead of refusing would deadlock here rather than merely run late. The bound
    // is kept as a coarse "did not block" net and is deliberately generous; at one
    // second it was measuring how loaded the runner was.
    assert!(overload_started.elapsed() < OBSERVE_BUDGET);
    assert!(overload
        .expect_err("overload")
        .to_string()
        .contains("overloaded"));
    prepare_release_tx.send(()).expect("release first job");
    let jobs = registry.runtime_job_registry().expect("jobs");
    assert_eq!(
        terminal(
            jobs.wait_terminal(&first_job, TERMINAL_BUDGET)
                .expect("wait first")
        )
        .state,
        RuntimeJobState::Succeeded
    );
    registry.shutdown(SHUTDOWN_GRACE).expect("shutdown");
}

#[test]
fn persistence_failure_closes_admission_marks_reconciling_and_checkpoint_retry_recovers() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("repo");
    let registry = ProjectBrainRegistry::with_capacity(temp.path().join("brains"), None, 2)
        .with_runtime_limits(2, 4);
    mount(&registry, &root);

    let baseline = registry
        .retry_runtime_checkpoint(&root.to_string_lossy())
        .expect("baseline checkpoint");
    let healthy = registry
        .runtime_health(&root.to_string_lossy())
        .expect("healthy runtime snapshot");
    assert_eq!(healthy.status, "healthy");
    assert_eq!(
        healthy.current_checkpoint_id.as_deref(),
        Some(baseline.checkpoint_id.as_str())
    );

    let graph_path = registry
        .read_runtime_snapshot(&root.to_string_lossy(), |state| {
            Ok::<_, RuntimeJobFailure>(state.graph_path.clone())
        })
        .expect("read graph path through actor")
        .value;
    let graph_backup = graph_path.with_extension("r6-persist-failure-backup");
    std::fs::rename(&graph_path, &graph_backup).expect("move graph out of persist path");
    std::fs::create_dir(&graph_path).expect("replace graph file with an unwritable directory");
    std::fs::write(graph_path.join("projection-blocker"), b"blocked")
        .expect("make the directory non-empty so authoritative projection refuses it");

    let failed_job = submit_root_append(
        &registry,
        &root,
        "persistence-failure",
        "visible-only-in-live-degraded-brain",
    );
    let jobs = registry.runtime_job_registry().expect("jobs");
    let failed = terminal(
        jobs.wait_terminal(&failed_job, TERMINAL_BUDGET)
            .expect("wait persistence failure"),
    );
    assert_eq!(failed.state, RuntimeJobState::Failed);
    assert_eq!(
        failed
            .terminal_result
            .as_ref()
            .map(|result| result.code.as_str()),
        Some("brain_checkpoint_committed_unconfirmed")
    );

    let degraded = registry
        .runtime_health(&root.to_string_lossy())
        .expect("degraded runtime health");
    assert_eq!(degraded.status, "reconciling");
    assert!(!degraded.accepting);
    assert!(degraded.degraded_persistence);
    assert!(!degraded.degraded_fallback);
    assert!(degraded.last_persistence_error.is_some());
    assert!(degraded.in_doubt_checkpoint_id.is_some());
    assert_eq!(registry.runtime_health_snapshots(), vec![degraded.clone()]);

    let read_error = registry
        .read_runtime_snapshot(&root.to_string_lossy(), |state| {
            Ok::<_, RuntimeJobFailure>(state.ingest_roots.clone())
        })
        .expect_err("an in-doubt CURRENT closes actor admission until reconciliation");
    assert!(
        read_error
            .to_string()
            .contains("degraded by persistence failure"),
        "unexpected read refusal: {read_error}"
    );
    assert!(
        graph_path.is_dir(),
        "the post-CURRENT projection obstruction remains until the external fault is cleared"
    );

    let blocked_apply_calls = Arc::new(AtomicUsize::new(0));
    let blocked_apply_counter = blocked_apply_calls.clone();
    let blocked = registry
        .submit_runtime_job(
            &root.to_string_lossy(),
            request(
                &registry,
                &root,
                "write-while-reconciling",
                degraded.version.revision,
            ),
            |_state| Ok::<_, RuntimeJobFailure>(()),
            |_context, _snapshot| Ok::<_, RuntimeJobFailure>(()),
            move |_state, ()| {
                blocked_apply_counter.fetch_add(1, Ordering::SeqCst);
                Ok(RuntimeJobSuccess::new("unexpected", "must not apply"))
            },
        )
        .expect_err("reconciling actor must refuse a new job before preparation");
    assert!(blocked
        .to_string()
        .contains("degraded by persistence failure"));
    assert_eq!(blocked_apply_calls.load(Ordering::SeqCst), 0);

    std::fs::remove_file(graph_path.join("projection-blocker")).expect("remove projection blocker");
    std::fs::remove_dir(&graph_path).expect("clear projection obstruction");
    // Poll the reconciliation OUT of the health surface; the loop exits on the first
    // non-reconciling read, so the budget only caps a stuck run.
    let deadline = Instant::now() + OBSERVE_BUDGET;
    while registry
        .runtime_health(&root.to_string_lossy())
        .expect("poll recovery health")
        .status
        == "reconciling"
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(20));
    }

    let recovered = registry
        .retry_runtime_checkpoint(&root.to_string_lossy())
        .expect("reconciled CURRENT yields an exact checkpoint ACK");
    let recovered_health = registry
        .runtime_health(&root.to_string_lossy())
        .expect("recovered runtime health");
    assert_eq!(recovered_health.status, "healthy");
    assert!(!recovered_health.degraded_persistence);
    assert!(recovered_health.last_persistence_error.is_none());
    assert_eq!(
        recovered_health.current_checkpoint_id.as_deref(),
        Some(recovered.checkpoint_id.as_str())
    );
    assert_ne!(
        recovered.checkpoint_id, baseline.checkpoint_id,
        "the post-CURRENT candidate becomes the authoritative new generation"
    );
    let recovered_roots = registry
        .read_runtime_snapshot(&root.to_string_lossy(), |state| {
            Ok::<_, RuntimeJobFailure>(state.ingest_roots.clone())
        })
        .expect("reconciled brain is readable")
        .value;
    assert!(
        recovered_roots
            .iter()
            .any(|value| value == "visible-only-in-live-degraded-brain"),
        "post-CURRENT recovery must expose the complete new generation"
    );
    assert!(
        graph_path.is_file(),
        "reconciliation restores the graph file"
    );

    let post_recovery_job = submit_root_append(
        &registry,
        &root,
        "write-after-persistence-recovery",
        "write-after-recovery",
    );
    assert_eq!(
        terminal(
            jobs.wait_terminal(&post_recovery_job, TERMINAL_BUDGET)
                .expect("wait post-recovery write")
        )
        .state,
        RuntimeJobState::Succeeded
    );
    std::fs::remove_file(graph_backup).expect("remove test-only graph backup");
    registry.shutdown(SHUTDOWN_GRACE).expect("shutdown");
}

#[test]
fn shutdown_ack_and_exact_current_restore_overwrite_corrupt_canonical_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("repo");
    let base = temp.path().join("brains");
    let canonical_root;
    let store_dir;
    let replay_job_id;
    {
        let registry = ProjectBrainRegistry::new(base.clone(), None);
        mount(&registry, &root);
        canonical_root = ProjectBrainRegistry::canonical_key(&root.to_string_lossy());
        let job = submit_root_append(&registry, &root, "restart-write", "durable-root");
        replay_job_id = job.clone();
        let jobs = registry.runtime_job_registry().expect("jobs");
        assert_eq!(
            terminal(jobs.wait_terminal(&job, TERMINAL_BUDGET).expect("wait")).state,
            RuntimeJobState::Succeeded
        );
        let acks = registry.shutdown(SHUTDOWN_GRACE).expect("shutdown ACK");
        assert_eq!(acks.len(), 1);
        assert_eq!(acks[0].brain_id, registry.brain_id_for(&canonical_root));
        store_dir = registry.store_dir_for(&canonical_root);
    }

    std::fs::write(store_dir.join("graph_snapshot.json"), b"{broken-json")
        .expect("corrupt canonical graph");
    std::fs::write(store_dir.join("ingest_roots.json"), b"[]").expect("erase canonical roots");

    let registry = ProjectBrainRegistry::new(base, None);
    registry
        .try_resolve(&canonical_root)
        .expect("checkpoint-backed restart")
        .expect("warm brain");
    let receipt = registry
        .recovery_receipt(&canonical_root)
        .expect("explicit recovery receipt");
    assert_eq!(receipt.disposition, CheckpointLoadDisposition::ExactCurrent);
    assert_eq!(
        receipt.authority_receipt.validator_id,
        "m1nd-project-brain-unbound-authority-v1"
    );
    let roots = registry
        .read_runtime_snapshot(&canonical_root, |state| {
            Ok::<_, RuntimeJobFailure>(state.ingest_roots.clone())
        })
        .expect("read restored roots")
        .value;
    assert!(roots.iter().any(|root| root == "durable-root"));
    assert_eq!(
        registry
            .runtime_job_registry()
            .expect("reopen durable jobs")
            .get(&replay_job_id)
            .expect("replayed terminal job")
            .state,
        RuntimeJobState::Succeeded
    );
    registry.shutdown(SHUTDOWN_GRACE).expect("shutdown");
}

#[test]
fn corrupt_latest_checkpoint_recovers_only_with_degraded_fallback_receipt() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("repo");
    let base = temp.path().join("brains");
    let canonical_root;
    let checkpoint_root;
    {
        let registry = ProjectBrainRegistry::new(base.clone(), None);
        mount(&registry, &root);
        canonical_root = ProjectBrainRegistry::canonical_key(&root.to_string_lossy());
        let jobs = registry.runtime_job_registry().expect("jobs");
        for (job_id, marker) in [("fallback-first", "first"), ("fallback-second", "second")] {
            let job = submit_root_append(&registry, &root, job_id, marker);
            assert_eq!(
                terminal(jobs.wait_terminal(&job, TERMINAL_BUDGET).expect("wait")).state,
                RuntimeJobState::Succeeded
            );
        }
        registry.shutdown(SHUTDOWN_GRACE).expect("shutdown");
        checkpoint_root = registry
            .store_dir_for(&canonical_root)
            .join(BRAIN_CHECKPOINT_DIRECTORY);
    }

    let pointer: serde_json::Value =
        serde_json::from_slice(&std::fs::read(checkpoint_root.join("CURRENT")).expect("CURRENT"))
            .expect("pointer json");
    let current_id = pointer["current_checkpoint_id"]
        .as_str()
        .expect("current id");
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            checkpoint_root
                .join("checkpoints")
                .join(current_id)
                .join("manifest.json"),
        )
        .expect("manifest"),
    )
    .expect("manifest json");
    let graph_blob = manifest["file_inventory"]
        .as_array()
        .expect("inventory")
        .iter()
        .find(|file| file["logical_name"] == GRAPH_SNAPSHOT_LOGICAL_NAME)
        .and_then(|file| file["blob_path"].as_str())
        .expect("graph blob");
    std::fs::write(
        checkpoint_root
            .join("checkpoints")
            .join(current_id)
            .join(graph_blob),
        b"corrupt-current-generation",
    )
    .expect("corrupt latest blob");

    let registry = ProjectBrainRegistry::new(base, None);
    registry
        .try_resolve(&canonical_root)
        .expect("fallback restart")
        .expect("brain");
    let receipt = registry
        .recovery_receipt(&canonical_root)
        .expect("fallback receipt");
    assert_eq!(
        receipt.disposition,
        CheckpointLoadDisposition::DegradedFallback
    );
    assert!(receipt.fallback_receipt.is_some());
    let roots = registry
        .read_runtime_snapshot(&canonical_root, |state| {
            Ok::<_, RuntimeJobFailure>(state.ingest_roots.clone())
        })
        .expect("degraded reads remain available")
        .value;
    assert!(roots.iter().any(|root| root == "first"));
    assert!(!roots.iter().any(|root| root == "second"));
    let revision = current_revision(&registry, &root);
    let refused = registry.submit_runtime_job(
        &canonical_root,
        request(&registry, &root, "degraded-write", revision),
        |_state| Ok::<_, RuntimeJobFailure>(()),
        |_context, _snapshot| Ok::<_, RuntimeJobFailure>(()),
        |_state, ()| Ok(RuntimeJobSuccess::new("unexpected", "must stay read only")),
    );
    // Submission is allowed to register, but commit must fail closed because the
    // fallback cannot be promoted over corrupt CURRENT by guesswork.
    let job_id = refused.expect("durably register degraded job");
    let job = terminal(
        registry
            .runtime_job_registry()
            .expect("jobs")
            .wait_terminal(&job_id, TERMINAL_BUDGET)
            .expect("wait degraded refusal"),
    );
    assert_eq!(job.state, RuntimeJobState::Failed);
    assert!(job
        .terminal_result
        .as_ref()
        .expect("terminal")
        .message
        .contains("CURRENT repair is required"));
}

#[test]
fn corrupt_current_pointer_fails_closed_instead_of_fresh_boot() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("repo");
    let base = temp.path().join("brains");
    let canonical_root;
    let checkpoint_root;
    {
        let registry = ProjectBrainRegistry::new(base.clone(), None);
        mount(&registry, &root);
        canonical_root = ProjectBrainRegistry::canonical_key(&root.to_string_lossy());
        let job = submit_root_append(&registry, &root, "pointer-write", "pointer-root");
        let jobs = registry.runtime_job_registry().expect("jobs");
        assert_eq!(
            terminal(jobs.wait_terminal(&job, TERMINAL_BUDGET).expect("wait")).state,
            RuntimeJobState::Succeeded
        );
        registry.shutdown(SHUTDOWN_GRACE).expect("shutdown");
        checkpoint_root = registry
            .store_dir_for(&canonical_root)
            .join(BRAIN_CHECKPOINT_DIRECTORY);
    }
    std::fs::write(checkpoint_root.join("CURRENT"), b"{not-json").expect("corrupt CURRENT pointer");

    let registry = ProjectBrainRegistry::new(base, None);
    let error = match registry.try_resolve(&canonical_root) {
        Ok(_) => panic!("corrupt CURRENT must not become a fresh brain"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("checkpoint_pointer_corrupt"));
    assert_eq!(registry.warm_len(), 0);
}
