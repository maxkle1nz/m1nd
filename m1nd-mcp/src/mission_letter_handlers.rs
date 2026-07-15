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
    /// The ORIGIN of an ARCHIVE gesture (F2.5e), validated ONLY when the letter's
    /// `phase` is `archived`. This is the INPUT token — exactly like `receipt_import`'s
    /// `imported_via` — and NEVER a field of the letter schema (§1f schema discipline).
    /// The owner's Human View screen stamps `"human-ui"`; a runner/agent never composes
    /// it, so an archive without it is refused `human_gesture_required`.
    #[serde(default)]
    pub archived_via: Option<String>,
}

/// The CLOSED allow-list of HUMAN origin tokens that pass the archive gate (F2.5e).
/// Today only the owner's web UI composes a legitimate archive gesture (`"human-ui"`) —
/// the SAME allow-list `receipt_import` carries. A new origin (a native tray gesture) is
/// a code change plus a test here, never a silently-trusted client string. Absent,
/// empty, or any off-list value is refused.
const ARCHIVE_HUMAN_ORIGINS: &[&str] = &["human-ui"];

/// The mailbox box for the bound brain — the SAME box the `kind=mission` read
/// serves. Mirrors `http_server::handle_mailbox`: the repo-side box when the brain
/// has a code root, else the medulla box (a memory-only brain). `north` reuses it
/// to ring the landing bell over the very box the tray and `mission_post` speak.
pub(crate) fn mission_box_path(state: &SessionState) -> PathBuf {
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
    // The ARCHIVE gate (F2.5e / binding change 1): archiving a superseded receipt is a
    // HUMAN gesture — the THIRD human write after `system_blocks_ratify` and
    // `receipt_import`. Armed ONLY when the letter is `archived`, the terminal "set this
    // superseded receipt aside" phase. This is the product's first SILENT-burial verb
    // (`failed` at least pins loud atop the tray; `archived` is quiet), so it MUST be the
    // human's own hand — never an agent silencing its own unproven work. The origin token
    // rides the INPUT (`archived_via`), never the letter schema, validated against a
    // CLOSED allow-list; absent/empty/off-list is refused `human_gesture_required` and
    // NOTHING is appended — a literal mirror of `handle_receipt_import`'s gate. Forgeable
    // on an unauthenticated loopback, so it closes the CHEAP reflex vector (an agent
    // calling `mission_post` with an archived letter), not a same-UID process (§5d). It
    // holds on BOTH seams (the MCP wire and REST) — both route through this one handler.
    if input.letter.phase == mission_letter::Phase::Archived {
        let origin = input.archived_via.as_deref().unwrap_or("");
        if !ARCHIVE_HUMAN_ORIGINS.contains(&origin) {
            return Ok(json!({
                "ok": false,
                "refused": "human_gesture_required",
                "tool": "mission_post",
                "field": "archived_via",
                "allowed_origins": ARCHIVE_HUMAN_ORIGINS,
                "lesson": "archiving a superseded receipt is the human gesture — the owner's screen sends it; agents never do",
            }));
        }
    }
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
    // The block guard (field-hardening, proposed by the hand agent after
    // mission_post silently accepted a letter naming a block that did not exist):
    // a real letter must name a real block in this brain's store. A legitimately
    // synthetic letter (a smoke/warm-pool probe) sets `synthetic: true` and skips
    // this — the escape hatch is explicit, never a silent pass. A brain with no
    // store yet (no skeleton) cannot validate, so it accepts (the honest "no
    // skeleton to check against" state, same spirit as the medulla brain_ref case).
    if !input.letter.synthetic {
        let store_dir = crate::system_blocks_handlers::store_dir(state);
        if let Some(store) =
            crate::system_blocks::SystemBlockStore::load(&store_dir).map_err(|e| {
                M1ndError::InvalidParams {
                    tool: "mission_post".to_string(),
                    detail: format!("could not read the system-block store: {e}"),
                }
            })?
        {
            // A SKELETON-scoped mission (the F11-c §3a curation dispatch) anchors
            // its letter at the store's skeleton id — a REAL identity of this
            // brain, so it validates like a block. Any OTHER skeleton/block id
            // still refuses (this recognizes the one true anchor, it does not
            // loosen the guard). Field bug 2026-07-10: the curation letter was
            // refused as unknown_block because the guard predated the anchor.
            let known = store
                .blocks
                .iter()
                .any(|b| b.block_id == input.letter.block_id)
                || store.skeleton.skeleton_id == input.letter.block_id;
            if !known {
                return Err(M1ndError::InvalidParams {
                    tool: "mission_post".to_string(),
                    detail: format!(
                        "unknown_block: the letter names block_id '{}' which no block in                          this brain's skeleton holds (and it is not this skeleton's own id) —                          name a real block, name the skeleton id for a skeleton-scoped                          mission, or set synthetic:true for a smoke probe; nothing was appended",
                        input.letter.block_id
                    ),
                });
            }
            // The boundary-staleness guard (field bug: the orphan letter
            // msn_17a1d1f9b013). A `receipt_candidate` is the one-click import the
            // tray offers; the §1 contract gate only proved its evidence was
            // COMPLETE (artifact_hash + evidence_refs), never that its
            // `scope.boundary_version` still matched the LIVE block. A candidate
            // proving a boundary the block has moved past is dead evidence —
            // `receipt_import` would reject it with `stale_scope`, yet the letter
            // was already appended (that is how the orphan was born). Declare the
            // staleness at POST, naming both versions, and append nothing. Mirrors
            // the import law (`system_blocks`: receipt.scope.boundary_version !=
            // block.boundary_version).
            if let Some(candidate) = &input.letter.receipt_candidate {
                if let Some(block) = store
                    .blocks
                    .iter()
                    .find(|b| b.block_id == candidate.block_id)
                {
                    if candidate.scope.boundary_version != block.boundary_version {
                        return Err(M1ndError::InvalidParams {
                            tool: "mission_post".to_string(),
                            detail: format!(
                                "stale_scope: the receipt candidate for block '{}' proves \
                                 boundary_version {} but the live block is at boundary_version {} \
                                 — re-earn the candidate against the live boundary; nothing was \
                                 appended",
                                candidate.block_id,
                                candidate.scope.boundary_version,
                                block.boundary_version
                            ),
                        });
                    }
                }
            }
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
