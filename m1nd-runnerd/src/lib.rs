//! `m1nd-runnerd` — the runner daemon (HUMAN-VIEW-V2-F2.5c §5). The ONLY spawner:
//! it executes pinned spawn missions in an isolated git worktree, runs the gate, and
//! emits mission letters on the chain — `judging → executing → (merge_wait | failed)`,
//! and NEVER `landed` (the import is a human act, §1d). It is a CLIENT of the owner's
//! frozen mission-letter contract; the owner's `mission_post` gates (the §1 contract,
//! the §1e head CAS, the §1d landed-law) apply to every letter it emits.
//!
//! The library exposes the testable heart (config parse, `/run` validation, the
//! engine); the binary ([`main`]) wires the loopback axum server, the boot announce,
//! and the shared secret.

pub mod config;
// F12 — the synchronous `/curate` curation lane (one propose call; never a mission,
// never a store write; the hand PROPOSES ops-as-data and the owner applies, §2/§3).
pub mod curation;
pub mod mission;
// F11-b — the synchronous `/name` naming lane (per-block runner calls; never a
// mission, never a store write; the daemon is transport + timeout + sanitize).
pub mod naming;
pub mod owner;
