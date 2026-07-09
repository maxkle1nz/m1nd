//! HUMAN VIEW v2 — F2.5a: the `mission_post` MCP verb (§2c).
//!
//! A thin wrapper over [`crate::mission_letter::post_mission_letter`]: it resolves
//! the mailbox box for the bound brain from the session (exactly the box the
//! `GET /api/mailbox?...&kind=mission` read serves — the repo-side box, or the
//! medulla box for a memory-only brain), calls the pure post engine, and maps its
//! honest refusals onto the MCP surface with the keyword in the detail
//! (`stale_head`, `invalid_phase`, `landed_law`, …). It NEVER opens the
//! SystemBlockStore — a mission letter is state, not evidence (§1c).
//!
//! WRITE verb: it is on the read-only-attach deny-list
//! (`server::READ_ONLY_DENIED_TOOLS`) alongside the store writers.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{json, Value};

use m1nd_core::error::{M1ndError, M1ndResult};

use crate::mission_letter::{self, MissionLetter, MissionLetterError};
use crate::session::SessionState;

/// The `mission_post` input (§2c): the emitting agent + the mission letter.
#[derive(Debug, Deserialize)]
pub struct MissionPostInput {
    /// The emitting agent id — stamped into the mailbox line (part of the content
    /// id, so an identical replay from the same agent dedups).
    pub agent_id: String,
    /// The mission letter (validated against §1 before anything is appended).
    pub letter: MissionLetter,
}

/// The mailbox box for the bound brain — the SAME box the `kind=mission` read
/// serves. Mirrors `http_server::handle_mailbox`: the repo-side box when the brain
/// has a code root, else the medulla box (a memory-only brain).
fn mission_box_path(state: &SessionState) -> PathBuf {
    match state.project_root_display() {
        Some(root) => Path::new(&root).join(crate::mailbox::BOX_REL_PATH),
        None => crate::mailbox::medulla_box_path(&state.runtime_root),
    }
}

/// Map a [`MissionLetterError`] onto the MCP error surface — every honest refusal
/// becomes an `InvalidParams` whose detail carries the keyword the caller acts on
/// (`stale_head`, `invalid_phase`, `landed_law`, `incomplete_evidence`, …).
/// Mirrors `system_blocks_handlers::seed_err`.
fn mission_err(err: MissionLetterError) -> M1ndError {
    M1ndError::InvalidParams {
        tool: "mission_post".to_string(),
        detail: err.to_string(),
    }
}

/// `mission_post` (WRITE, §2c). Validates the §1 contract (schema + per-phase
/// gating incl. the 1d landed law), computes the mission's current head from the
/// box, runs the §1e head CAS, and appends the letter as a `kind=mission` mailbox
/// line — reusing the mailbox append/dedup. A stale head returns `stale_head` and
/// nothing is appended; an identical replay dedups (idempotent).
pub fn handle_mission_post(state: &mut SessionState, input: MissionPostInput) -> M1ndResult<Value> {
    // The brain guard (field-hardening, proposed by the first external hand agent
    // after living the trap): a letter whose `brain_ref` does not name the brain
    // THIS session is bound to would land silently in the WRONG box — the exact
    // mis-route that hit both the hand agent and the tray on day one. Refuse
    // honestly instead. `brain_ref` matches the bound brain's display name (the
    // §4A.9 echo identity). A medulla-bound session (no code root) has no display
    // and accepts any ref — the memory-only box is an explicit fallback, not a
    // mis-route.
    if let Some(bound) = state.code_root_display_name() {
        if input.letter.brain_ref != bound {
            return Err(M1ndError::InvalidParams {
                tool: "mission_post".to_string(),
                detail: format!(
                    "brain_mismatch: the letter names brain_ref '{}' but this session is bound \
                     to '{}' — bind to the right brain (ingest project_root=<its root>) or fix \
                     the letter; nothing was appended",
                    input.letter.brain_ref, bound
                ),
            });
        }
    }
    let box_path = mission_box_path(state);
    let outcome = mission_letter::post_mission_letter(&box_path, &input.agent_id, &input.letter)
        .map_err(mission_err)?;
    Ok(json!({
        "letter_id": outcome.letter_id,
        "mission_id": outcome.mission_id,
        "mission_seq": outcome.mission_seq,
        "deduped": outcome.deduped,
        "phase": serde_json::to_value(input.letter.phase).unwrap_or(Value::Null),
    }))
}
