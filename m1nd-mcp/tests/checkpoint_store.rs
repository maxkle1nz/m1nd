// This contract test compiles the production module directly so private
// checkpoint invariants remain testable; unrelated private helpers are expected.
#![allow(dead_code)]

#[cfg(windows)]
#[path = "../src/windows_durable_fs.rs"]
mod windows_durable_fs;

#[path = "../src/checkpoint_store.rs"]
mod checkpoint_store;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Duration;

use checkpoint_store::{
    external_authority_refs_digest, CheckpointAuthorityValidationReceiptV1,
    CheckpointAuthorityValidator, CheckpointCreateV1, CheckpointError,
    CheckpointExternalAuthorityRefsV1, CheckpointFaultEvent, CheckpointFaultInjector,
    CheckpointFaultPoint, CheckpointFileInputV1, CheckpointGcPolicyV1, CheckpointLoadDisposition,
    CheckpointManifestV1, CheckpointStore, InjectedCheckpointFault, NoCheckpointFaults,
    GRAPH_SNAPSHOT_LOGICAL_NAME, INGEST_ROOTS_LOGICAL_NAME,
};

static CURRENT_DIRECTORY_TEST_LOCK: Mutex<()> = Mutex::new(());

struct CurrentDirectoryGuard(PathBuf);

impl CurrentDirectoryGuard {
    fn capture() -> Self {
        Self(std::env::current_dir().expect("current directory before test"))
    }
}

impl Drop for CurrentDirectoryGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).expect("restore current directory after test");
    }
}

fn digest(byte: char) -> String {
    std::iter::repeat_n(byte, 64).collect()
}

fn input(generation: u64, previous: Option<String>) -> CheckpointCreateV1 {
    let mut schema_versions = BTreeMap::new();
    schema_versions.insert("graph-schema".to_string(), "v7".to_string());
    schema_versions.insert("roots-schema".to_string(), "v3".to_string());
    schema_versions.insert("sidecar-schema".to_string(), "v2".to_string());
    CheckpointCreateV1 {
        brain_id: "brain-test".to_string(),
        epoch: 1,
        generation,
        revision: generation * 10,
        schema_versions,
        files: vec![
            CheckpointFileInputV1 {
                logical_name: GRAPH_SNAPSHOT_LOGICAL_NAME.to_string(),
                relative_path: "graph_snapshot.json".to_string(),
                schema_id: "graph-schema".to_string(),
                schema_version: "v7".to_string(),
                bytes: format!("graph-generation-{generation}").into_bytes(),
            },
            CheckpointFileInputV1 {
                logical_name: INGEST_ROOTS_LOGICAL_NAME.to_string(),
                relative_path: "ingest_roots.json".to_string(),
                schema_id: "roots-schema".to_string(),
                schema_version: "v3".to_string(),
                bytes: format!("roots-generation-{generation}").into_bytes(),
            },
            CheckpointFileInputV1 {
                logical_name: "temporal_sidecar".to_string(),
                relative_path: "sidecars/temporal.json".to_string(),
                schema_id: "sidecar-schema".to_string(),
                schema_version: "v2".to_string(),
                bytes: format!("temporal-generation-{generation}").into_bytes(),
            },
        ],
        external_authority_refs: CheckpointExternalAuthorityRefsV1 {
            system_block_store_version: generation,
            mission_heads_index_digest: digest('a'),
            authority_wal_root_digest: digest('b'),
            intent_core_store_root_digest: digest('c'),
            sentinel_outbox_watermark_digest: digest('d'),
            autonomy_epoch_record_digest: digest('e'),
        },
        created_at_unix_ms: 10_000 + generation,
        expected_current_checkpoint_id: previous,
    }
}

struct ExactAuthorityValidator;

impl CheckpointAuthorityValidator for ExactAuthorityValidator {
    fn validate(
        &self,
        manifest: &CheckpointManifestV1,
        refs_digest: &str,
    ) -> Result<CheckpointAuthorityValidationReceiptV1, String> {
        assert_eq!(
            external_authority_refs_digest(&manifest.external_authority_refs).expect("refs digest"),
            refs_digest
        );
        CheckpointAuthorityValidationReceiptV1::verified(
            "test-authority-validator",
            &manifest.checkpoint_id,
            refs_digest,
            digest('f'),
            50_000 + manifest.generation,
        )
        .map_err(|error| error.to_string())
    }
}

struct RejectCheckpointValidator {
    rejected: String,
}

impl CheckpointAuthorityValidator for RejectCheckpointValidator {
    fn validate(
        &self,
        manifest: &CheckpointManifestV1,
        refs_digest: &str,
    ) -> Result<CheckpointAuthorityValidationReceiptV1, String> {
        if manifest.checkpoint_id == self.rejected {
            return Err("protected authority root rejected current checkpoint".to_string());
        }
        ExactAuthorityValidator.validate(manifest, refs_digest)
    }
}

struct FailAtPoint {
    point: CheckpointFaultPoint,
    seen: AtomicUsize,
    code: &'static str,
}

impl FailAtPoint {
    fn once(point: CheckpointFaultPoint) -> Self {
        Self {
            point,
            seen: AtomicUsize::new(0),
            code: "SIMULATED_FAULT",
        }
    }

    fn disk_full(point: CheckpointFaultPoint) -> Self {
        Self {
            point,
            seen: AtomicUsize::new(0),
            code: "ENOSPC_SIMULATED",
        }
    }
}

impl CheckpointFaultInjector for FailAtPoint {
    fn check(&self, event: &CheckpointFaultEvent) -> Result<(), InjectedCheckpointFault> {
        if event.point == self.point && self.seen.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(InjectedCheckpointFault::new(
                self.code,
                format!("fault at {:?}", event.point),
            ));
        }
        Ok(())
    }
}

#[test]
fn checkpoint_round_trip_is_content_addressed_and_ack_binds_eviction_revision() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = CheckpointStore::open(temp.path().join("store")).expect("store");
    assert_eq!(
        store.root(),
        temp.path()
            .canonicalize()
            .expect("canonical tempdir")
            .join("store")
    );
    let ack = store
        .create_checkpoint(input(1, None), &NoCheckpointFaults)
        .expect("checkpoint");
    assert_eq!(ack.checkpoint_id.len(), 64);
    assert_eq!(ack.schema, checkpoint_store::CHECKPOINT_ACK_SCHEMA);
    assert_eq!(ack.epoch, 1);
    assert!(ack.confirmed_at_unix_ms > 0);
    let pointer = store.current_pointer().expect("pointer");
    assert_eq!(pointer.current_checkpoint_id, ack.checkpoint_id);
    assert_eq!(pointer.fallback_checkpoint_id, None);

    let loaded = store
        .load_current(&ExactAuthorityValidator)
        .expect("load current");
    assert_eq!(loaded.disposition, CheckpointLoadDisposition::ExactCurrent);
    assert_eq!(
        loaded.authority_receipt.schema,
        checkpoint_store::CHECKPOINT_AUTHORITY_RECEIPT_SCHEMA
    );
    assert_eq!(loaded.manifest.generation, 1);
    assert_eq!(
        loaded
            .read_file(GRAPH_SNAPSHOT_LOGICAL_NAME)
            .expect("graph"),
        b"graph-generation-1"
    );
    let permit = ack
        .eviction_permit("brain-test", 1, 1, 10)
        .expect("eviction permit");
    assert_eq!(permit.checkpoint_id, ack.checkpoint_id);
    assert_eq!(permit.epoch, 1);
    assert!(serde_json::to_value(&ack).expect("serialize ACK")["checkpoint_id"].is_string());
    assert!(
        serde_json::to_value(&permit).expect("serialize permit")["current_pointer_digest"]
            .is_string()
    );
    assert!(matches!(
        ack.eviction_permit("brain-test", 1, 1, 11),
        Err(CheckpointError::EvictionAckMismatch { .. })
    ));
    assert!(matches!(
        ack.eviction_permit("brain-test", 2, 1, 10),
        Err(CheckpointError::EvictionAckMismatch { .. })
    ));

    let second_temp = tempfile::tempdir().expect("tempdir");
    let second_store =
        CheckpointStore::open(second_temp.path().join("store")).expect("second store");
    let same = second_store
        .create_checkpoint(input(1, None), &NoCheckpointFaults)
        .expect("same content checkpoint");
    assert_eq!(same.checkpoint_id, ack.checkpoint_id);
}

#[test]
fn relative_root_remains_bound_after_current_directory_changes() {
    let _serial = CURRENT_DIRECTORY_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _current_directory = CurrentDirectoryGuard::capture();
    let temp = tempfile::tempdir().expect("tempdir");
    let opening_directory = temp.path().join("opening-directory");
    let later_directory = temp.path().join("later-directory");
    std::fs::create_dir_all(&opening_directory).expect("opening directory");
    std::fs::create_dir_all(&later_directory).expect("later directory");
    std::env::set_current_dir(&opening_directory).expect("enter opening directory");

    let store = CheckpointStore::open("relative/store").expect("relative store");
    let expected_root = opening_directory
        .join("relative")
        .canonicalize()
        .expect("normalized parent")
        .join("store");
    assert!(store.root().is_absolute());
    assert_eq!(store.root(), expected_root);

    std::env::set_current_dir(&later_directory).expect("change current directory after open");
    let ack = store
        .create_checkpoint(input(1, None), &NoCheckpointFaults)
        .expect("checkpoint remains anchored");
    let directory = store.checkpoint_directory(&ack.checkpoint_id);
    assert!(directory.starts_with(&expected_root));
    assert!(directory.exists());
    assert!(!later_directory.join("relative").exists());
}

#[test]
fn corrupt_latest_requires_explicit_degraded_fallback_receipt() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = CheckpointStore::open(temp.path().join("store")).expect("store");
    let first = store
        .create_checkpoint(input(1, None), &NoCheckpointFaults)
        .expect("first");
    let second = store
        .create_checkpoint(
            input(2, Some(first.checkpoint_id.clone())),
            &NoCheckpointFaults,
        )
        .expect("second");
    let current = store
        .load_current(&ExactAuthorityValidator)
        .expect("current");
    let graph = current
        .manifest
        .file_inventory
        .iter()
        .find(|file| file.logical_name == GRAPH_SNAPSHOT_LOGICAL_NAME)
        .expect("graph inventory");
    std::fs::write(current.directory().join(&graph.blob_path), b"corrupt")
        .expect("corrupt latest blob");

    assert!(matches!(
        store.load_current(&ExactAuthorityValidator),
        Err(CheckpointError::DigestMismatch { .. })
    ));
    assert!(matches!(
        store.load_with_fallback(&ExactAuthorityValidator, 0),
        Err(CheckpointError::Refused {
            code: "checkpoint_fallback_receipt_time_missing",
            ..
        })
    ));
    let degraded = store
        .load_with_fallback(&ExactAuthorityValidator, 99_000)
        .expect("explicit fallback");
    assert_eq!(
        degraded.disposition,
        CheckpointLoadDisposition::DegradedFallback
    );
    assert_eq!(degraded.manifest.checkpoint_id, first.checkpoint_id);
    let receipt = degraded.fallback_receipt.expect("fallback receipt");
    assert_eq!(receipt.requested_checkpoint_id, second.checkpoint_id);
    assert_eq!(receipt.selected_checkpoint_id, first.checkpoint_id);
    assert_eq!(receipt.receipt_digest.len(), 64);
}

#[test]
fn authority_rejection_of_latest_can_fall_back_but_never_passes_as_exact() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = CheckpointStore::open(temp.path().join("store")).expect("store");
    let first = store
        .create_checkpoint(input(1, None), &NoCheckpointFaults)
        .expect("first");
    let second = store
        .create_checkpoint(
            input(2, Some(first.checkpoint_id.clone())),
            &NoCheckpointFaults,
        )
        .expect("second");
    let validator = RejectCheckpointValidator {
        rejected: second.checkpoint_id.clone(),
    };
    assert!(matches!(
        store.load_current(&validator),
        Err(CheckpointError::AuthorityValidation(_))
    ));
    let fallback = store
        .load_with_fallback(&validator, 101_000)
        .expect("fallback");
    assert_eq!(fallback.manifest.checkpoint_id, first.checkpoint_id);
    assert_eq!(
        fallback.disposition,
        CheckpointLoadDisposition::DegradedFallback
    );
}

#[test]
fn corrupt_current_pointer_fails_closed_without_scanning_complete_directories() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = CheckpointStore::open(temp.path().join("store")).expect("store");
    let first = store
        .create_checkpoint(input(1, None), &NoCheckpointFaults)
        .expect("first");
    let second = store
        .create_checkpoint(
            input(2, Some(first.checkpoint_id.clone())),
            &NoCheckpointFaults,
        )
        .expect("second");
    assert!(store.checkpoint_directory(&first.checkpoint_id).exists());
    assert!(store.checkpoint_directory(&second.checkpoint_id).exists());

    std::fs::write(store.root().join("CURRENT"), b"not a valid pointer").expect("corrupt CURRENT");
    assert!(matches!(
        store.load_with_fallback(&ExactAuthorityValidator, 102_000),
        Err(CheckpointError::PointerCorrupt(_))
    ));
}

#[test]
fn checksummed_current_with_wrong_predecessor_is_semantically_rejected() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = CheckpointStore::open(temp.path().join("store")).expect("store");
    let first = store
        .create_checkpoint(input(1, None), &NoCheckpointFaults)
        .expect("first");
    let second_input = input(2, Some(first.checkpoint_id.clone()));
    let second = store
        .create_checkpoint(second_input.clone(), &NoCheckpointFaults)
        .expect("second");
    let mut pointer = store.current_pointer().expect("pointer");
    pointer.fallback_checkpoint_id = Some(digest('9'));
    let core = serde_json::json!({
        "schema": &pointer.schema,
        "pointer_revision": pointer.pointer_revision,
        "current_checkpoint_id": &pointer.current_checkpoint_id,
        "fallback_checkpoint_id": &pointer.fallback_checkpoint_id,
        "previous_pointer_digest": &pointer.previous_pointer_digest,
    });
    pointer.pointer_digest = m1nd_control::digest_canonical("m1nd-checkpoint-current-v1", &core)
        .expect("recompute structurally valid pointer digest");
    std::fs::write(
        store.root().join("CURRENT"),
        serde_json::to_vec_pretty(&pointer).expect("pointer JSON"),
    )
    .expect("rewrite pointer");

    assert!(matches!(
        store.load_current(&ExactAuthorityValidator),
        Err(CheckpointError::PointerCorrupt(_))
    ));
    assert!(matches!(
        store.load_with_fallback(&ExactAuthorityValidator, 102_500),
        Err(CheckpointError::PointerCorrupt(_))
    ));
    assert!(matches!(
        store.create_checkpoint(second_input, &NoCheckpointFaults),
        Err(CheckpointError::PointerCorrupt(_))
    ));
    assert_eq!(
        store
            .current_pointer()
            .expect("checksummed pointer remains structurally readable")
            .current_checkpoint_id,
        second.checkpoint_id
    );
}

#[test]
fn corrupt_current_and_fallback_return_no_usable_checkpoint() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = CheckpointStore::open(temp.path().join("store")).expect("store");
    let first = store
        .create_checkpoint(input(1, None), &NoCheckpointFaults)
        .expect("first");
    let second = store
        .create_checkpoint(
            input(2, Some(first.checkpoint_id.clone())),
            &NoCheckpointFaults,
        )
        .expect("second");

    for checkpoint_id in [&first.checkpoint_id, &second.checkpoint_id] {
        let checkpoint = store
            .load_current(&ExactAuthorityValidator)
            .or_else(|_| store.load_with_fallback(&ExactAuthorityValidator, 103_000))
            .expect("one selected generation remains valid while corrupting");
        let target = if checkpoint.manifest.checkpoint_id == *checkpoint_id {
            checkpoint
        } else {
            let directory = store.checkpoint_directory(checkpoint_id);
            let manifest: CheckpointManifestV1 = serde_json::from_slice(
                &std::fs::read(directory.join("manifest.json")).expect("manifest bytes"),
            )
            .expect("manifest");
            let file = manifest
                .file_inventory
                .iter()
                .find(|file| file.logical_name == GRAPH_SNAPSHOT_LOGICAL_NAME)
                .expect("graph file");
            std::fs::write(directory.join(&file.blob_path), b"corrupt fallback")
                .expect("corrupt non-selected generation");
            continue;
        };
        let file = target
            .manifest
            .file_inventory
            .iter()
            .find(|file| file.logical_name == GRAPH_SNAPSHOT_LOGICAL_NAME)
            .expect("graph file");
        std::fs::write(target.directory().join(&file.blob_path), b"corrupt current")
            .expect("corrupt selected generation");
    }

    assert!(matches!(
        store.load_with_fallback(&ExactAuthorityValidator, 104_000),
        Err(CheckpointError::NoUsableCheckpoint { .. })
    ));
}

#[test]
fn every_persistence_fault_leaves_current_old_or_new_never_partial() {
    let fault_points = [
        CheckpointFaultPoint::CreateStagingDirectory,
        CheckpointFaultPoint::CreateBlobDirectory,
        CheckpointFaultPoint::WriteBlob,
        CheckpointFaultPoint::FsyncBlob,
        CheckpointFaultPoint::FsyncBlobDirectory,
        CheckpointFaultPoint::WriteManifest,
        CheckpointFaultPoint::FsyncManifest,
        CheckpointFaultPoint::FsyncStagingDirectory,
        CheckpointFaultPoint::RenameCheckpointDirectory,
        CheckpointFaultPoint::FsyncCheckpointParent,
        CheckpointFaultPoint::WriteCurrent,
        CheckpointFaultPoint::FsyncCurrent,
        CheckpointFaultPoint::RenameCurrent,
        CheckpointFaultPoint::FsyncCurrentParent,
        CheckpointFaultPoint::ConfirmCurrent,
    ];

    for point in fault_points {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = CheckpointStore::open(temp.path().join("store")).expect("store");
        let old = store
            .create_checkpoint(input(1, None), &NoCheckpointFaults)
            .expect("baseline");
        let error = store
            .create_checkpoint(
                input(2, Some(old.checkpoint_id.clone())),
                &FailAtPoint::once(point),
            )
            .expect_err("fault must suppress ACK");
        assert!(matches!(error, CheckpointError::Injected { .. }));
        let loaded = store
            .load_current(&ExactAuthorityValidator)
            .expect("CURRENT is old or new and complete");
        assert!(
            loaded.manifest.generation == 1 || loaded.manifest.generation == 2,
            "{point:?} selected partial generation {}",
            loaded.manifest.generation
        );
        if matches!(
            point,
            CheckpointFaultPoint::FsyncCurrentParent | CheckpointFaultPoint::ConfirmCurrent
        ) {
            assert_eq!(loaded.manifest.generation, 2, "{point:?} follows rename");
        } else {
            assert_eq!(
                loaded.manifest.generation, 1,
                "{point:?} precedes CURRENT rename"
            );
        }
    }
}

#[test]
fn simulated_disk_full_never_acknowledges_or_advances_current() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = CheckpointStore::open(temp.path().join("store")).expect("store");
    let old = store
        .create_checkpoint(input(1, None), &NoCheckpointFaults)
        .expect("baseline");
    let error = store
        .create_checkpoint(
            input(2, Some(old.checkpoint_id.clone())),
            &FailAtPoint::disk_full(CheckpointFaultPoint::WriteBlob),
        )
        .expect_err("disk full");
    assert!(matches!(
        error,
        CheckpointError::Injected { ref code, .. } if code == "ENOSPC_SIMULATED"
    ));
    assert_eq!(
        store
            .current_pointer()
            .expect("pointer")
            .current_checkpoint_id,
        old.checkpoint_id
    );
}

#[test]
fn post_current_fsync_failure_has_no_ack_but_idempotent_retry_confirms_it() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = CheckpointStore::open(temp.path().join("store")).expect("store");
    let old = store
        .create_checkpoint(input(1, None), &NoCheckpointFaults)
        .expect("baseline");
    let next_input = input(2, Some(old.checkpoint_id));
    let error = store
        .create_checkpoint(
            next_input.clone(),
            &FailAtPoint::once(CheckpointFaultPoint::FsyncCurrentParent),
        )
        .expect_err("no ACK before parent fsync");
    assert!(matches!(error, CheckpointError::Injected { .. }));
    assert_eq!(
        store
            .load_current(&ExactAuthorityValidator)
            .expect("new pointer structurally complete")
            .manifest
            .generation,
        2
    );
    let recovered_ack = store
        .create_checkpoint(next_input, &NoCheckpointFaults)
        .expect("idempotent confirmation retry");
    assert_eq!(recovered_ack.generation, 2);
    recovered_ack
        .eviction_permit("brain-test", 1, 2, 20)
        .expect("permit only after recovered ACK");
}

#[test]
fn open_removes_only_unpublished_staging_and_current_temporaries() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("store");
    let checkpoints = root.join("checkpoints");
    std::fs::create_dir_all(checkpoints.join(".staging-dead-process")).expect("stale staging");
    std::fs::write(
        checkpoints.join(".staging-dead-process").join("partial"),
        b"partial",
    )
    .expect("partial staging file");
    std::fs::write(root.join(".CURRENT.tmp-dead-process"), b"partial pointer")
        .expect("stale CURRENT temporary");
    std::fs::write(
        root.join("operator-note"),
        b"not owned by checkpoint cleanup",
    )
    .expect("unrelated root entry");

    let store = CheckpointStore::open(&root).expect("recovered store");
    assert!(!checkpoints.join(".staging-dead-process").exists());
    assert!(!root.join(".CURRENT.tmp-dead-process").exists());
    assert_eq!(
        std::fs::read(root.join("operator-note")).expect("unrelated entry preserved"),
        b"not owned by checkpoint cleanup"
    );
    assert!(matches!(
        store.current_pointer(),
        Err(CheckpointError::PointerMissing)
    ));
}

#[test]
fn gc_preserves_current_fallback_legacy_predecessor_and_explicit_protection() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = CheckpointStore::open(temp.path().join("store")).expect("store");
    let first = store
        .create_checkpoint(input(1, None), &NoCheckpointFaults)
        .expect("first");
    let second = store
        .create_checkpoint(
            input(2, Some(first.checkpoint_id.clone())),
            &NoCheckpointFaults,
        )
        .expect("second");
    let third = store
        .create_checkpoint(
            input(3, Some(second.checkpoint_id.clone())),
            &NoCheckpointFaults,
        )
        .expect("third");
    let fourth = store
        .create_checkpoint(
            input(4, Some(third.checkpoint_id.clone())),
            &NoCheckpointFaults,
        )
        .expect("fourth");

    let receipt = store
        .gc(&CheckpointGcPolicyV1 {
            retain_newest_additional: 0,
            protected_checkpoint_ids: BTreeSet::from([first.checkpoint_id.clone()]),
        })
        .expect("gc");
    assert!(store.checkpoint_directory(&fourth.checkpoint_id).exists());
    assert!(store.checkpoint_directory(&third.checkpoint_id).exists());
    assert!(store.checkpoint_directory(&first.checkpoint_id).exists());
    assert!(store.checkpoint_directory(&second.checkpoint_id).exists());
    assert!(receipt
        .preserved_checkpoint_ids
        .contains(&fourth.checkpoint_id));
    assert!(receipt
        .preserved_checkpoint_ids
        .contains(&third.checkpoint_id));
    assert!(receipt
        .preserved_checkpoint_ids
        .contains(&second.checkpoint_id));
    assert!(receipt.receipt_digest.len() == 64);
}

struct PauseAtRename {
    entered: Arc<Barrier>,
    release: Arc<Barrier>,
}

impl CheckpointFaultInjector for PauseAtRename {
    fn check(&self, event: &CheckpointFaultEvent) -> Result<(), InjectedCheckpointFault> {
        if event.point == CheckpointFaultPoint::RenameCheckpointDirectory {
            self.entered.wait();
            self.release.wait();
        }
        Ok(())
    }
}

#[test]
fn concurrent_gc_serializes_and_preserves_new_current_plus_fallback() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = CheckpointStore::open(temp.path().join("store")).expect("store");
    let first = store
        .create_checkpoint(input(1, None), &NoCheckpointFaults)
        .expect("first");
    let second = store
        .create_checkpoint(input(2, Some(first.checkpoint_id)), &NoCheckpointFaults)
        .expect("second");

    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let create_store = store.clone();
    let create_entered = Arc::clone(&entered);
    let create_release = Arc::clone(&release);
    let second_id = second.checkpoint_id.clone();
    let creator = thread::spawn(move || {
        create_store.create_checkpoint(
            input(3, Some(second_id)),
            &PauseAtRename {
                entered: create_entered,
                release: create_release,
            },
        )
    });
    entered.wait();

    let gc_finished = Arc::new(AtomicBool::new(false));
    let gc_flag = Arc::clone(&gc_finished);
    let gc_store = store.clone();
    let gc = thread::spawn(move || {
        let result = gc_store.gc(&CheckpointGcPolicyV1 {
            retain_newest_additional: 0,
            protected_checkpoint_ids: BTreeSet::new(),
        });
        gc_flag.store(true, Ordering::Release);
        result
    });
    thread::sleep(Duration::from_millis(30));
    assert!(!gc_finished.load(Ordering::Acquire));
    release.wait();
    let third = creator.join().expect("creator").expect("third checkpoint");
    gc.join().expect("gc thread").expect("gc");

    let pointer = store.current_pointer().expect("pointer");
    assert_eq!(pointer.current_checkpoint_id, third.checkpoint_id);
    assert_eq!(
        pointer.fallback_checkpoint_id.as_deref(),
        Some(second.checkpoint_id.as_str())
    );
    assert!(store
        .checkpoint_directory(&pointer.current_checkpoint_id)
        .exists());
    assert!(store
        .checkpoint_directory(pointer.fallback_checkpoint_id.as_ref().expect("fallback"))
        .exists());
}

#[cfg(unix)]
#[test]
fn single_writer_and_symlink_paths_are_refused() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("store");
    let first = CheckpointStore::open(&root).expect("first writer");
    assert!(matches!(
        CheckpointStore::open(&root),
        Err(CheckpointError::WriterLocked(_))
    ));
    std::fs::remove_file(root.join("WRITER.lock")).expect("replaceable legacy lock inode");
    assert!(matches!(
        CheckpointStore::open(&root),
        Err(CheckpointError::WriterLocked(_))
    ));
    drop(first);
    let reopened = CheckpointStore::open(&root).expect("directory lease released");
    drop(reopened);

    let target = temp.path().join("real-store");
    std::fs::create_dir(&target).expect("target");
    let linked = temp.path().join("linked-store");
    symlink(&target, &linked).expect("symlink");
    assert!(matches!(
        CheckpointStore::open(&linked),
        Err(CheckpointError::SymlinkRefused(_))
    ));

    let checkpoint_root = temp.path().join("checkpoint-store");
    let checkpoint_store = CheckpointStore::open(&checkpoint_root).expect("checkpoint store");
    checkpoint_store
        .create_checkpoint(input(1, None), &NoCheckpointFaults)
        .expect("checkpoint");
    let loaded = checkpoint_store
        .load_current(&ExactAuthorityValidator)
        .expect("load checkpoint");
    let graph = loaded
        .manifest
        .file_inventory
        .iter()
        .find(|file| file.logical_name == GRAPH_SNAPSHOT_LOGICAL_NAME)
        .expect("graph inventory");
    let blob = loaded.directory().join(&graph.blob_path);
    let outside = temp.path().join("outside-blob");
    std::fs::write(&outside, b"graph-generation-1").expect("outside blob");
    std::fs::remove_file(&blob).expect("remove original blob");
    symlink(&outside, &blob).expect("replace blob with symlink");
    assert!(matches!(
        checkpoint_store.load_current(&ExactAuthorityValidator),
        Err(CheckpointError::SymlinkRefused(_))
    ));

    let traversal_root = temp.path().join("traversal-store");
    let traversal_store = CheckpointStore::open(&traversal_root).expect("traversal store");
    let mut traversal = input(1, None);
    traversal.files[0].relative_path = "../escape".to_string();
    assert!(matches!(
        traversal_store.create_checkpoint(traversal, &NoCheckpointFaults),
        Err(CheckpointError::Refused {
            code: "checkpoint_path_traversal_refused",
            ..
        })
    ));
    assert!(!traversal_root.join("CURRENT").exists());

    let stale_root = temp.path().join("stale-symlink-store");
    let stale_checkpoints = stale_root.join("checkpoints");
    std::fs::create_dir_all(&stale_checkpoints).expect("stale checkpoint root");
    symlink(&target, stale_checkpoints.join(".staging-hostile-symlink"))
        .expect("stale staging symlink");
    assert!(matches!(
        CheckpointStore::open(&stale_root),
        Err(CheckpointError::SymlinkRefused(_))
    ));
    let _windows_boundary = CheckpointError::PlatformNotProven("Windows primitive not proven");
}

#[cfg(unix)]
#[test]
fn renamed_and_recreated_root_cannot_split_writer_namespace() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("store");
    let displaced_root = temp.path().join("store.old");
    let first = CheckpointStore::open(&root).expect("first writer");
    let first_ack = first
        .create_checkpoint(input(1, None), &NoCheckpointFaults)
        .expect("first checkpoint");
    let loaded = first
        .load_current(&ExactAuthorityValidator)
        .expect("loaded checkpoint");

    std::fs::rename(&root, &displaced_root).expect("displace original root");
    std::fs::create_dir(&root).expect("recreate replacement root");

    // The lease is in the parent namespace, so replacing the root inode cannot
    // mint a second writer while the first store is alive.
    assert!(matches!(
        CheckpointStore::open(&root),
        Err(CheckpointError::WriterLocked(_))
    ));

    // Every store/read boundary checks the original root dev+inode binding, so
    // an operation begun after the replacement never crosses into `root`.
    for error in [
        first
            .current_pointer()
            .expect_err("A must refuse replacement"),
        loaded
            .read_file(GRAPH_SNAPSHOT_LOGICAL_NAME)
            .expect_err("loaded read must refuse replacement"),
        first
            .create_checkpoint(input(2, Some(first_ack.checkpoint_id)), &NoCheckpointFaults)
            .expect_err("write must refuse replacement"),
    ] {
        assert!(matches!(
            error,
            CheckpointError::Refused {
                code: "checkpoint_root_binding_changed",
                ..
            }
        ));
    }

    assert_eq!(
        std::fs::read_dir(&root).expect("replacement root").count(),
        0,
        "neither writer may populate the replacement root"
    );
    assert!(displaced_root.join("CURRENT").is_file());
    assert!(displaced_root.join("checkpoints").is_dir());
}

#[cfg(windows)]
#[test]
fn locked_gc_tombstone_never_blocks_store_open() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path().join("checkpoint-root");
    let tombstone = root
        .join("checkpoints")
        .join(format!(".gc-{}-held", digest('a')));
    std::fs::create_dir_all(&tombstone).expect("tombstone directory");
    let held_path = tombstone.join("held-by-reader");
    let held = windows_durable_fs::open_lock_file_no_follow(&held_path)
        .expect("open a handle that denies delete sharing");

    let store = CheckpointStore::open(&root)
        .expect("physical tombstone reclamation must not block canonical store open");
    assert!(
        tombstone.is_dir(),
        "locked tombstone should remain for retry"
    );

    drop(store);
    drop(held);
    std::fs::remove_dir_all(tombstone).expect("cleanup unlocked tombstone");
}
