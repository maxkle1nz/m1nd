//! Protected anti-rollback anchors for append-only owner journals.
//!
//! Hash chaining detects internal edits but not replacement with an older valid
//! prefix. Production therefore pins each journal's exact `(sequence, head)`
//! in an independently protected compare-and-swap backend. An append whose
//! journal fsync succeeds but protected CAS does not is deliberately
//! availability-fatal: reopening observes a mismatch and refuses to infer.

use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

pub const OWNER_AUTHORIZATION_BROKER_HEAD_DOMAIN: &str = "m1nd-owner-authorization-broker-head-v1";
pub const AUTHORITY_WAL_HEAD_DOMAIN: &str = "m1nd-authority-wal-head-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProtectedJournalHeadAssuranceV1 {
    SoftwareTestOnlyNotProven,
    HardwareProtectedAttested,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedJournalHeadSnapshotV1 {
    pub domain: String,
    pub record_sequence: u64,
    pub head_digest: Option<String>,
}

impl ProtectedJournalHeadSnapshotV1 {
    pub fn observed(domain: &str, record_sequence: u64, head_digest: Option<String>) -> Self {
        Self {
            domain: domain.to_string(),
            record_sequence,
            head_digest,
        }
    }
}

pub trait ProtectedJournalHeadBackendV1: Send {
    fn assurance(&self) -> ProtectedJournalHeadAssuranceV1;
    fn read_latest(&self, domain: &str) -> Result<Option<ProtectedJournalHeadSnapshotV1>, String>;
    fn compare_and_advance(
        &mut self,
        domain: &str,
        expected: Option<&ProtectedJournalHeadSnapshotV1>,
        next: &ProtectedJournalHeadSnapshotV1,
    ) -> Result<(), String>;
}

pub type SharedProtectedJournalHeadBackendV1 = Arc<Mutex<Box<dyn ProtectedJournalHeadBackendV1>>>;

/// Explicit software backend for deterministic batteries. Production assembly
/// rejects this assurance before creating any durable runtime root.
#[derive(Clone, Default)]
pub struct SoftwareTestProtectedJournalHeadBackendV1 {
    state: Arc<Mutex<BTreeMap<String, ProtectedJournalHeadSnapshotV1>>>,
}

impl SoftwareTestProtectedJournalHeadBackendV1 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared(self) -> SharedProtectedJournalHeadBackendV1 {
        Arc::new(Mutex::new(Box::new(self)))
    }

    pub fn snapshot(&self, domain: &str) -> Option<ProtectedJournalHeadSnapshotV1> {
        self.state.lock().get(domain).cloned()
    }
}

impl ProtectedJournalHeadBackendV1 for SoftwareTestProtectedJournalHeadBackendV1 {
    fn assurance(&self) -> ProtectedJournalHeadAssuranceV1 {
        ProtectedJournalHeadAssuranceV1::SoftwareTestOnlyNotProven
    }

    fn read_latest(&self, domain: &str) -> Result<Option<ProtectedJournalHeadSnapshotV1>, String> {
        Ok(self.state.lock().get(domain).cloned())
    }

    fn compare_and_advance(
        &mut self,
        domain: &str,
        expected: Option<&ProtectedJournalHeadSnapshotV1>,
        next: &ProtectedJournalHeadSnapshotV1,
    ) -> Result<(), String> {
        if next.domain != domain {
            return Err("protected journal-head domain mismatch".to_string());
        }
        let mut state = self.state.lock();
        if state.get(domain) != expected {
            return Err("protected journal-head compare-and-swap mismatch".to_string());
        }
        match expected {
            None if next.record_sequence != 0 || next.head_digest.is_some() => {
                return Err("initial protected journal head must be the empty anchor".to_string());
            }
            Some(previous)
                if next.record_sequence != previous.record_sequence.saturating_add(1)
                    || next.head_digest.is_none() =>
            {
                return Err("protected journal head must advance exactly one record".to_string());
            }
            _ => {}
        }
        state.insert(domain.to_string(), next.clone());
        Ok(())
    }
}

pub(crate) fn verify_or_initialize_protected_head(
    backend: &SharedProtectedJournalHeadBackendV1,
    domain: &str,
    observed_sequence: u64,
    observed_head_digest: Option<String>,
) -> Result<ProtectedJournalHeadSnapshotV1, String> {
    let observed =
        ProtectedJournalHeadSnapshotV1::observed(domain, observed_sequence, observed_head_digest);
    let mut backend = backend.lock();
    let protected = backend.read_latest(domain)?;
    match protected {
        Some(protected) if protected == observed => Ok(protected),
        Some(protected) => Err(format!(
            "protected head differs from journal: protected={protected:?}, observed={observed:?}"
        )),
        None if observed.record_sequence == 0 && observed.head_digest.is_none() => {
            backend.compare_and_advance(domain, None, &observed)?;
            Ok(observed)
        }
        None => Err("non-empty journal has no protected anti-rollback head".to_string()),
    }
}

pub(crate) fn advance_protected_head(
    backend: &SharedProtectedJournalHeadBackendV1,
    domain: &str,
    expected: &ProtectedJournalHeadSnapshotV1,
    next_sequence: u64,
    next_head_digest: String,
) -> Result<ProtectedJournalHeadSnapshotV1, String> {
    let next =
        ProtectedJournalHeadSnapshotV1::observed(domain, next_sequence, Some(next_head_digest));
    let mut backend = backend.lock();
    let current = backend.read_latest(domain)?;
    if current.as_ref() != Some(expected) {
        return Err("protected journal head changed outside owner serial".to_string());
    }
    backend.compare_and_advance(domain, Some(expected), &next)?;
    Ok(next)
}
