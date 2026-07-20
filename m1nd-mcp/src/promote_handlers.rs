// === m1nd-mcp/src/promote_handlers.rs ===
//
// `promote` verb — the audited crossing from a project-private claim UP into the
// medulla (the shared doctrine store), MEDULLA-PRD §7 + ORGANISM-PRD §C8.2/§C8.3.
//
// Promotion ELEVATES, never moves (MED-INV-3): the medulla gets a COPY stamped
// with full origin provenance; the project original stays in place, stamped as a
// witness (`Promoted-To`). Demotion un-shares (learn-wrong / consolidation on the
// medulla copy) and NEVER touches the witness — see the module doc `demotion`
// note below and MEDULLA-PRD §7.
//
// This is an OWNER-LEVEL cross-store operation: it reads a project brain's store
// and writes into the medulla store. A single `&mut SessionState` handler cannot
// reach two stores, so the routing seam (`mcp_http::route_and_run`) resolves the
// source store dir + the medulla store dir and calls [`promote_claim`] with both.
// The pure logic here is store-path agnostic and unit-testable without a server.

use crate::light_author_handlers::{
    render_light_markdown, LightAuthorInput, LightClaim, LockGuard, SupersessionOutcome,
};
use crate::util::now_ms;
use m1nd_core::error::{M1ndError, M1ndResult};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// Input for the `promote` MCP verb (MEDULLA-PRD §7):
/// `promote { agent_id, brain: <project_root>, claim: <slug>, reason: <one line> }`.
#[derive(Debug, Clone)]
pub struct PromoteInput {
    /// The calling agent — recorded as `Promoted-By` (etiquette-by-provenance,
    /// TT-INV-7: any id may call, every promotion is auditably attributed).
    pub agent_id: String,
    /// The source project root whose store holds the claim (the `Origin-Brain`).
    pub brain: String,
    /// The slug of the claim to promote (its `.light.md` basename without suffix).
    pub claim: String,
    /// One line recorded as `Promotion-Reason` — WHY this is transversal.
    pub reason: String,
}

// ---------------------------------------------------------------------------
// The C8.3 evidence-class gate (ORGANISM-PRD §C8.3, P3)
// ---------------------------------------------------------------------------

/// The promotion evidence-class gate (§C8.3): a claim may promote only if its
/// `State:` is `verified` OR its `Source-Agent:` is `human:maintainer`. Declared
/// maker findings stop one verb short of every session's doctrine beat. Returns
/// `Ok(())` when the claim qualifies, else a typed refusal reason.
fn evidence_class_gate(frontmatter: &ClaimFrontmatter) -> Result<(), String> {
    let state_ok = frontmatter
        .state
        .as_deref()
        .map(|s| s.trim().eq_ignore_ascii_case("verified"))
        .unwrap_or(false);
    let founder_ok = frontmatter
        .source_agent
        .as_deref()
        .map(|a| a.trim().eq_ignore_ascii_case("human:maintainer"))
        .unwrap_or(false);
    if state_ok || founder_ok {
        Ok(())
    } else {
        Err(format!(
            "promotion refused (C8.3 evidence-class gate): only a claim with State: verified \
             or Source-Agent: human:maintainer may promote — this claim is State: {} / \
             Source-Agent: {}. Verify it in its home brain first, then promote.",
            frontmatter.state.as_deref().unwrap_or("unknown"),
            frontmatter.source_agent.as_deref().unwrap_or("unknown"),
        ))
    }
}

// ---------------------------------------------------------------------------
// The hygiene floor (MEDULLA-PRD §7 step 2 / §7.1)
// ---------------------------------------------------------------------------

/// The promotion hygiene floor: the medulla is the CLEANEST store (read by every
/// session's beat), so a claim carrying a secret or a merge-conflict marker is
/// refused at the door. A deliberately conservative, dependency-free scan — the
/// same posture as the write-door secret guard the maintainer's hook enforces.
/// Returns `Ok(())` when clean, else the reason the claim is refused.
fn hygiene_floor(text: &str) -> Result<(), String> {
    // Conflict-marker guard: a claim carrying an unresolved merge conflict is
    // never doctrine.
    for marker in ["<<<<<<<", "=======", ">>>>>>>"] {
        if text.lines().any(|l| l.starts_with(marker)) {
            return Err(format!(
                "promotion refused (hygiene floor): the claim text carries a merge-conflict \
                 marker ('{marker}') — resolve it in the source brain before promoting."
            ));
        }
    }
    // Secret guard: high-signal credential shapes. Conservative substrings — a
    // false positive costs one re-word; a leaked secret in the most-read store is
    // the crime the floor exists to prevent.
    const SECRET_MARKERS: &[&str] = &[
        "-----BEGIN ",       // PEM private-key blocks
        "aws_secret_access", // AWS secret keys
        "AKIA",              // AWS access-key id prefix
        "sk-",               // OpenAI / generic secret-key prefix
        "ghp_",              // GitHub personal access token
        "xoxb-",             // Slack bot token
        "xoxp-",             // Slack user token
    ];
    let lower = text.to_ascii_lowercase();
    for marker in SECRET_MARKERS {
        // Case-insensitive for the word-ish markers; the token prefixes are
        // matched case-sensitively enough by the lowered scan (they are lowercase
        // or contain a digit/uppercase run that survives loosely — err toward
        // refusal, never toward leaking).
        if lower.contains(&marker.to_ascii_lowercase()) {
            return Err(format!(
                "promotion refused (hygiene floor): the claim text matches a secret shape \
                 ('{}') — the medulla is the most-read store and must never carry a credential. \
                 Redact it in the source brain before promoting.",
                marker.trim()
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Frontmatter + claim parsing (read a project `.light.md` back into an input)
// ---------------------------------------------------------------------------

/// The frontmatter fields the promotion path needs off the source claim.
#[derive(Debug, Default, Clone)]
pub struct ClaimFrontmatter {
    pub node: Option<String>,
    pub state: Option<String>,
    pub source_agent: Option<String>,
    pub origin_brain: Option<String>,
}

/// A parsed source claim: its frontmatter + reconstructed claim blocks (enough to
/// re-render an identical body via [`render_light_markdown`]).
pub struct ParsedClaim {
    pub frontmatter: ClaimFrontmatter,
    pub title: Option<String>,
    pub claims: Vec<LightClaim>,
}

/// Parse a `.light.md` document into its frontmatter + claim blocks. A tolerant,
/// line-oriented scan (the same fallback posture the supersession scanner uses):
/// it reads the frontmatter keys the promotion path needs and reconstructs each
/// entity/state/event marker with its epistemic qualifiers and evidence. It does
/// NOT need to be a full L1GHT parser — only to round-trip the fields promotion
/// carries (label, kind, text, confidence, ambiguity, evidence, depends_on).
pub fn parse_light_claim(text: &str) -> ParsedClaim {
    let mut fm = ClaimFrontmatter::default();
    let mut title: Option<String> = None;
    let mut claims: Vec<LightClaim> = Vec::new();

    let mut in_frontmatter = false;
    let mut frontmatter_done = false;
    // The prose line most recently seen (becomes the next marker's `text`).
    let mut pending_prose: Option<String> = None;

    for raw in text.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();

        // Frontmatter fence handling.
        if trimmed == "---" {
            if !in_frontmatter && !frontmatter_done {
                in_frontmatter = true;
            } else if in_frontmatter {
                in_frontmatter = false;
                frontmatter_done = true;
            }
            continue;
        }
        if in_frontmatter {
            if let Some(v) = trimmed.strip_prefix("Node:") {
                fm.node = Some(v.trim().to_string());
            } else if let Some(v) = trimmed.strip_prefix("State:") {
                fm.state = Some(v.trim().to_string());
            } else if let Some(v) = trimmed.strip_prefix("Source-Agent:") {
                fm.source_agent = Some(v.trim().to_string());
            } else if let Some(v) = trimmed.strip_prefix("Origin-Brain:") {
                fm.origin_brain = Some(v.trim().to_string());
            }
            continue;
        }

        // Body.
        if let Some(v) = trimmed.strip_prefix("## ") {
            if title.is_none() {
                title = Some(v.trim().to_string());
            }
            pending_prose = None;
            continue;
        }
        if trimmed.starts_with("# ") {
            // The `# <node>` title line — ignored (node comes from frontmatter).
            continue;
        }

        // A marker line: `[⍂ entity: label]`, `[⍐ state: label]`, `[⍌ event: label]`.
        if let Some((kind, label)) = parse_entity_marker(trimmed) {
            claims.push(LightClaim {
                label,
                text: pending_prose.take(),
                kind: Some(kind.to_string()),
                confidence: None,
                ambiguity: None,
                evidence: Vec::new(),
                depends_on: Vec::new(),
            });
            continue;
        }
        // Epistemic / relation qualifiers attach to the most-recent claim.
        if let Some(rest) = trimmed.strip_prefix("[𝔻 confidence:") {
            if let Some(c) = claims.last_mut() {
                c.confidence = Some(rest.trim_end_matches(']').trim().to_string());
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("[𝔻 ambiguity:") {
            if let Some(c) = claims.last_mut() {
                c.ambiguity = Some(rest.trim_end_matches(']').trim().to_string());
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("[𝔻 evidence:") {
            if let Some(c) = claims.last_mut() {
                c.evidence
                    .push(rest.trim_end_matches(']').trim().to_string());
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("[⟁ depends_on:") {
            if let Some(c) = claims.last_mut() {
                c.depends_on
                    .push(rest.trim_end_matches(']').trim().to_string());
            }
            continue;
        }

        // A non-empty, non-marker line is prose for the NEXT marker.
        if !trimmed.is_empty() {
            pending_prose = Some(trimmed.to_string());
        }
    }

    ParsedClaim {
        frontmatter: fm,
        title,
        claims,
    }
}

/// Parse an entity/state/event marker line, returning `(kind_word, label)`.
fn parse_entity_marker(line: &str) -> Option<(&'static str, String)> {
    for (glyph, kind) in [("⍂", "entity"), ("⍐", "state"), ("⍌", "event")] {
        let prefix = format!("[{glyph} {kind}:");
        if let Some(rest) = line.strip_prefix(&prefix) {
            let label = rest.trim_end_matches(']').trim().to_string();
            return Some((kind, label));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// C8.2 — promotion re-anchors evidence (ORGANISM-PRD §C8.2, step 2.5)
// ---------------------------------------------------------------------------

/// The result of re-anchoring a promoted claim's evidence (§C8.2).
pub struct EvidenceReanchor {
    /// The claims with their evidence rewritten origin-qualified.
    pub claims: Vec<LightClaim>,
    /// True when the claim carried code evidence that was origin-qualified —
    /// freshness delegates back to the origin brain (channel (a)).
    pub origin_qualified: bool,
    /// True when the claim must be stamped `evidence_unverifiable` — it carried
    /// evidence but no resolvable origin root to qualify against (channel (b)):
    /// a medulla claim never reads fresher than it can prove.
    pub evidence_unverifiable: bool,
}

/// Rewrite a promoted claim's evidence paths **origin-qualified** (§C8.2 step 2.5):
/// `evidence: <path>` → `evidence: <origin_root>#<path>`, so medulla-side freshness
/// can delegate re-hashing back to the origin brain (channel (a)). When there is no
/// usable origin root to qualify against (the claim carries evidence but the origin
/// brain is unknown/unreachable), the claim is marked `evidence_unverifiable`
/// (channel (b)) — the honest declared-tissue state. A claim with NO evidence is
/// neither: it was never verified tissue, so there is nothing to re-anchor.
///
/// `origin_root` is the source brain's project root (its `Origin-Brain`), or `None`
/// when the source is itself the medulla / has no resolvable root.
pub fn reanchor_evidence(claims: &[LightClaim], origin_root: Option<&str>) -> EvidenceReanchor {
    let has_evidence = claims.iter().any(|c| !c.evidence.is_empty());
    if !has_evidence {
        // Declared tissue already — no evidence paths to re-anchor, nothing to mark.
        return EvidenceReanchor {
            claims: claims.to_vec(),
            origin_qualified: false,
            evidence_unverifiable: false,
        };
    }
    match origin_root {
        Some(root) if !root.trim().is_empty() && root != "medulla" => {
            // Channel (a): origin-qualify every evidence path so freshness can be
            // delegated back to the origin brain. Already-qualified paths (carry a
            // '#') are left as-is (idempotent re-promotion).
            let rewritten = claims
                .iter()
                .map(|c| {
                    let mut c2 = c.clone();
                    c2.evidence = c
                        .evidence
                        .iter()
                        .map(|e| {
                            if e.contains('#') {
                                e.clone()
                            } else {
                                format!("{root}#{e}")
                            }
                        })
                        .collect();
                    c2
                })
                .collect();
            EvidenceReanchor {
                claims: rewritten,
                origin_qualified: true,
                evidence_unverifiable: false,
            }
        }
        // Channel (b): no resolvable origin root — the evidence cannot be qualified
        // to anything freshness could delegate to. Mark the claim unverifiable and
        // carry the raw paths (as declared context, never as a freshness anchor).
        _ => EvidenceReanchor {
            claims: claims.to_vec(),
            origin_qualified: false,
            evidence_unverifiable: true,
        },
    }
}

/// Exact deterministic postimages for the authority-bound promotion adapter.
/// The plan is pure: it applies every promotion gate and provenance transform
/// but performs no write. The external transaction durably stages these bytes
/// and seals their digests before spending authority.
#[derive(Clone, Debug)]
pub(crate) struct ExternalPromotionPlanV1 {
    pub source_slug: String,
    pub medulla_slug: String,
    pub source_postimage: String,
    pub medulla_postimage: String,
    pub source_history: String,
    pub medulla_history: Option<String>,
    pub origin_brain: String,
    pub origin_qualified: bool,
    pub evidence_unverifiable: bool,
    pub promoted_at_ms: u64,
}

pub(crate) fn plan_external_promotion(
    input: &PromoteInput,
    source_text: &str,
    medulla_text: Option<&str>,
    promoted_at_ms: u64,
) -> M1ndResult<ExternalPromotionPlanV1> {
    if promoted_at_ms == 0 {
        return Err(M1ndError::InvalidParams {
            tool: "promote".to_string(),
            detail: "promotion requires a positive owner timestamp".to_string(),
        });
    }
    let source_slug = crate::light_author_handlers::slugify(&input.claim);
    let parsed = parse_light_claim(source_text);
    evidence_class_gate(&parsed.frontmatter).map_err(|detail| M1ndError::InvalidParams {
        tool: "promote".into(),
        detail,
    })?;
    hygiene_floor(source_text).map_err(|detail| M1ndError::InvalidParams {
        tool: "promote".into(),
        detail,
    })?;
    let origin_brain = parsed
        .frontmatter
        .origin_brain
        .clone()
        .filter(|origin| !origin.trim().is_empty())
        .unwrap_or_else(|| input.brain.clone());
    let reanchor = reanchor_evidence(&parsed.claims, Some(origin_brain.as_str()));
    let node_label = parsed
        .frontmatter
        .node
        .clone()
        .unwrap_or_else(|| input.claim.clone());
    let medulla_slug = crate::light_author_handlers::slugify(&node_label);
    let mut promoted_input = LightAuthorInput {
        agent_id: input.agent_id.clone(),
        node_label,
        title: parsed.title,
        state: parsed.frontmatter.state,
        claims: reanchor.claims,
        namespace: None,
        ingest_after: false,
        mode: "merge".into(),
        supersedes: None,
        origin_brain: Some(origin_brain.clone()),
        origin_claim: Some(source_slug.clone()),
        promoted_by: Some(input.agent_id.clone()),
        promotion_reason: Some(input.reason.clone()),
        promoted_to: None,
        evidence_unverifiable: reanchor.evidence_unverifiable,
        soul_source: None,
    };
    let (medulla_postimage, superseded) =
        crate::light_author_handlers::render_light_memory_superseding_candidate(
            &mut promoted_input,
            medulla_text,
            promoted_at_ms,
        )
        .map_err(|reason| M1ndError::InvalidParams {
            tool: "promote".to_string(),
            detail: format!(
                "promotion refused ({reason}): a stronger medulla claim '{medulla_slug}' is already live"
            ),
        })?;
    let source_postimage = insert_or_replace_frontmatter(
        source_text,
        "Promoted-To",
        &format!("medulla@{medulla_slug}@{promoted_at_ms}"),
    );
    let medulla_history = if superseded {
        Some(flip_state_to_outdated_for_plan(medulla_text.ok_or_else(
            || M1ndError::InvalidParams {
                tool: "promote".to_string(),
                detail: "supersession plan lost its medulla preimage".to_string(),
            },
        )?))
    } else {
        None
    };
    Ok(ExternalPromotionPlanV1 {
        source_slug,
        medulla_slug,
        source_postimage,
        medulla_postimage,
        source_history: flip_state_to_outdated_for_plan(source_text),
        medulla_history,
        origin_brain,
        origin_qualified: reanchor.origin_qualified,
        evidence_unverifiable: reanchor.evidence_unverifiable,
        promoted_at_ms,
    })
}

fn flip_state_to_outdated_for_plan(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut flipped = false;
    for line in text.lines() {
        if !flipped && line.trim_start().starts_with("State:") {
            output.push_str("State: outdated");
            flipped = true;
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }
    output
}

// ---------------------------------------------------------------------------
// The promote logic (pure over the two resolved store dirs)
// ---------------------------------------------------------------------------

/// The outcome of a promotion (a structured result the routing seam shapes to JSON).
pub struct PromoteOutcome {
    pub medulla_path: PathBuf,
    pub witness_path: PathBuf,
    pub medulla_slug: String,
    pub origin_brain: String,
    pub origin_qualified: bool,
    pub evidence_unverifiable: bool,
    pub medulla_claim_count: usize,
    pub soft_cap: usize,
}

/// Exact shared store locks held by the typed external promotion transaction.
/// The same `.locks/<slug>.lock` primitive is used by ordinary L1GHT writes, so
/// a source rewrite or medulla supersession cannot slip between OCC
/// revalidation and post-authority publication.
pub(crate) struct PromoteTargetLocksV1 {
    source_store_dir: PathBuf,
    medulla_store_dir: PathBuf,
    source_slug: String,
    medulla_slug: String,
    _guards: Vec<LockGuard>,
}

impl PromoteTargetLocksV1 {
    fn validates(
        &self,
        source_store_dir: &Path,
        medulla_store_dir: &Path,
        source_slug: &str,
        medulla_slug: &str,
    ) -> bool {
        self.source_store_dir == source_store_dir
            && self.medulla_store_dir == medulla_store_dir
            && self.source_slug == source_slug
            && self.medulla_slug == medulla_slug
    }
}

/// Acquire both promotion targets in canonical lock-path order. Keeping the
/// guards in one token makes the caller's critical section explicit and avoids
/// AB/BA deadlocks when two stores are promoted concurrently.
pub(crate) fn acquire_promote_target_locks(
    source_store_dir: &Path,
    source_slug: &str,
    medulla_store_dir: &Path,
    medulla_slug: &str,
) -> M1ndResult<PromoteTargetLocksV1> {
    let mut targets = vec![
        (source_store_dir.join(".locks"), source_slug.to_string()),
        (medulla_store_dir.join(".locks"), medulla_slug.to_string()),
    ];
    targets.sort_by(|left, right| {
        left.0
            .join(format!("{}.lock", left.1))
            .cmp(&right.0.join(format!("{}.lock", right.1)))
    });
    if targets[0].0 == targets[1].0 && targets[0].1 == targets[1].1 {
        return Err(M1ndError::InvalidParams {
            tool: "promote".to_string(),
            detail: "source and medulla promotion targets resolve to the same lock".to_string(),
        });
    }
    let mut guards = Vec::with_capacity(2);
    for (locks_dir, slug) in targets {
        guards.push(LockGuard::acquire_in(&locks_dir, &slug)?);
    }
    Ok(PromoteTargetLocksV1 {
        source_store_dir: source_store_dir.to_path_buf(),
        medulla_store_dir: medulla_store_dir.to_path_buf(),
        source_slug: source_slug.to_string(),
        medulla_slug: medulla_slug.to_string(),
        _guards: guards,
    })
}

/// The medulla soft cap (TT §6): the doctor warns at 300 active claims.
pub const MEDULLA_SOFT_CAP: usize = 300;

/// Execute a promotion. `source_store_dir` is the source brain's `agent-memory`
/// dir; `medulla_store_dir` is the medulla's `agent-memory` dir; `medulla_runtime_root`
/// is the medulla store's runtime root (parent of `agent-memory`, hosting the write
/// path and the supersession `.history`/`.locks`). Pure filesystem — no server, no
/// lock beyond the memorize path's own per-slug flock.
///
/// The sequence (MEDULLA-PRD §7):
///
/// - Step 1: load the claim from the source store (hard error on unknown slug).
/// - Step 2: the C8.3 evidence-class gate — verified OR founder-sourced (else refuse).
/// - Step 3: the hygiene floor — secret + conflict-marker scan (else refuse).
/// - Step 2.5 (C8.2): re-anchor evidence origin-qualified, OR mark evidence_unverifiable.
/// - Step 4: write the medulla copy through the memorize path — same supersession
///   gate (a weaker re-promotion bounces as WouldDowngrade), stamping the four
///   provenance fields.
/// - Step 5: stamp the project witness `Promoted-To: medulla@<slug>@<ms>` (a
///   same-strength supersession of the witness — `.history/` keeps the original).
/// - Step 6: return both paths + the medulla claim count vs the soft cap.
pub fn promote_claim(
    input: &PromoteInput,
    source_store_dir: &Path,
    medulla_store_dir: &Path,
    medulla_runtime_root: &Path,
) -> M1ndResult<PromoteOutcome> {
    promote_claim_impl(
        input,
        source_store_dir,
        medulla_store_dir,
        medulla_runtime_root,
        None,
    )
}

/// Execute promotion while the caller holds the exact source and destination
/// locks returned by [`acquire_promote_target_locks`].
pub(crate) fn promote_claim_with_target_locks(
    input: &PromoteInput,
    source_store_dir: &Path,
    medulla_store_dir: &Path,
    medulla_runtime_root: &Path,
    target_locks: &PromoteTargetLocksV1,
) -> M1ndResult<PromoteOutcome> {
    promote_claim_impl(
        input,
        source_store_dir,
        medulla_store_dir,
        medulla_runtime_root,
        Some(target_locks),
    )
}

fn promote_claim_impl(
    input: &PromoteInput,
    source_store_dir: &Path,
    medulla_store_dir: &Path,
    medulla_runtime_root: &Path,
    target_locks: Option<&PromoteTargetLocksV1>,
) -> M1ndResult<PromoteOutcome> {
    // 1. Load the source claim (hard error on unknown slug — no guessing).
    let source_slug = crate::light_author_handlers::slugify(&input.claim);
    let source_path = source_store_dir.join(format!("{source_slug}.light.md"));
    if !source_path.exists() {
        return Err(M1ndError::InvalidParams {
            tool: "promote".into(),
            detail: format!(
                "no claim '{}' (slug '{source_slug}') in brain '{}' — nothing to promote (no guessing on an unknown slug).",
                input.claim, input.brain
            ),
        });
    }
    let source_text = std::fs::read_to_string(&source_path).map_err(M1ndError::Io)?;
    let parsed = parse_light_claim(&source_text);

    // 2. C8.3 evidence-class gate — verified OR founder-sourced.
    evidence_class_gate(&parsed.frontmatter).map_err(|detail| M1ndError::InvalidParams {
        tool: "promote".into(),
        detail,
    })?;

    // 3. Hygiene floor — secret + conflict-marker scan.
    hygiene_floor(&source_text).map_err(|detail| M1ndError::InvalidParams {
        tool: "promote".into(),
        detail,
    })?;

    // The origin brain: the claim's own Origin-Brain stamp if present (the honest
    // where-born), else the caller-named source brain. A project claim always
    // carries its root post-M5a; legacy claims fall back to the named brain.
    let origin_brain = parsed
        .frontmatter
        .origin_brain
        .clone()
        .filter(|o| !o.trim().is_empty())
        .unwrap_or_else(|| input.brain.clone());

    // 2.5 (C8.2) Re-anchor evidence origin-qualified, or mark evidence_unverifiable.
    let reanchor = reanchor_evidence(&parsed.claims, Some(origin_brain.as_str()));

    // 4. Write the medulla copy through the memorize path. Same node label as the
    //    source (so re-promotion supersedes the same medulla slug, not a fork).
    let node_label = parsed
        .frontmatter
        .node
        .clone()
        .unwrap_or_else(|| input.claim.clone());
    let medulla_slug = crate::light_author_handlers::slugify(&node_label);
    let medulla_path = medulla_store_dir.join(format!("{medulla_slug}.light.md"));
    if target_locks.is_some_and(|locks| {
        !locks.validates(
            source_store_dir,
            medulla_store_dir,
            &source_slug,
            &medulla_slug,
        )
    }) {
        return Err(M1ndError::InvalidParams {
            tool: "promote".to_string(),
            detail: "held promotion locks do not bind the exact source and destination".to_string(),
        });
    }

    // The medulla copy keeps the source claim's State (already ≥ verified by the
    // gate) so re-promotion strength comparisons stay honest.
    let mut promoted_input = LightAuthorInput {
        agent_id: input.agent_id.clone(),
        node_label: node_label.clone(),
        title: parsed.title.clone(),
        state: parsed.frontmatter.state.clone(),
        claims: reanchor.claims.clone(),
        namespace: None,
        ingest_after: false,
        mode: "merge".into(),
        supersedes: None,
        origin_brain: Some(origin_brain.clone()),
        // The four promotion-provenance stamps (§7 · §6). The medulla copy carries
        // the readable chain: born in <Origin-Brain> as <Origin-Claim>, promoted by
        // <Promoted-By> for <Promotion-Reason>.
        origin_claim: Some(source_slug.clone()),
        promoted_by: Some(input.agent_id.clone()),
        promotion_reason: Some(input.reason.clone()),
        promoted_to: None,
        evidence_unverifiable: reanchor.evidence_unverifiable,
        soul_source: None,
    };

    // Write through the supersession-aware medulla write path so a weaker
    // re-promotion of an existing medulla claim bounces as WouldDowngrade.
    let write = if target_locks.is_some() {
        crate::light_author_handlers::write_light_memory_superseding_with_lock_held(
            &mut promoted_input,
            &medulla_path,
            medulla_runtime_root,
        )?
    } else {
        crate::light_author_handlers::write_light_memory_superseding(
            &mut promoted_input,
            &medulla_path,
            medulla_runtime_root,
        )?
    };
    if let SupersessionOutcome::WouldDowngrade { reason } = write {
        return Err(M1ndError::InvalidParams {
            tool: "promote".into(),
            detail: format!(
                "promotion refused ({reason}): a stronger medulla claim '{medulla_slug}' is already \
                 live — a weaker re-promotion is bounced (the shared doctrine keeps its strongest \
                 form). Supersede it in its home brain to a higher state/confidence first."
            ),
        });
    }

    // 5. Stamp the project witness `Promoted-To: medulla@<slug>@<ms>` — a
    //    same-strength supersession of the witness file (the `.history/` keeps the
    //    pre-promotion original). The witness stays IN PLACE (promotion elevates,
    //    never moves — MED-INV-3).
    let promoted_to_stamp = format!("medulla@{medulla_slug}@{}", now_ms());
    stamp_witness(
        &source_path,
        &source_slug,
        source_store_dir,
        &promoted_to_stamp,
    )?;

    // 6. Report the medulla claim count vs the soft cap.
    let medulla_claim_count = count_live_claims(medulla_store_dir);

    Ok(PromoteOutcome {
        medulla_path,
        witness_path: source_path,
        medulla_slug,
        origin_brain,
        origin_qualified: reanchor.origin_qualified,
        evidence_unverifiable: reanchor.evidence_unverifiable,
        medulla_claim_count,
        soft_cap: MEDULLA_SOFT_CAP,
    })
}

/// Stamp the project witness with `Promoted-To: <stamp>` — a same-strength
/// supersession that retains the pre-promotion original in `.history/`. The live
/// witness file is rewritten with the added frontmatter line; the prior copy is
/// archived flipped to `outdated` (the existing invalidate-and-keep audit trail).
fn stamp_witness(witness_path: &Path, slug: &str, store_dir: &Path, stamp: &str) -> M1ndResult<()> {
    let text = std::fs::read_to_string(witness_path).map_err(M1ndError::Io)?;
    // Archive the pre-promotion original (flipped to outdated) — the audit trail.
    crate::light_author_handlers::archive_prior_as_outdated_in(store_dir, witness_path, slug)?;
    // Insert `Promoted-To:` into the frontmatter (idempotent — replace if present).
    let stamped = insert_or_replace_frontmatter(&text, "Promoted-To", stamp);
    crate::light_author_handlers::write_atomic_managed_store(store_dir, witness_path, &stamped)?;
    Ok(())
}

/// Insert (or replace) a single frontmatter `Key: value` line inside the leading
/// `---` fence, preserving everything else byte-for-byte. If the key exists it is
/// replaced in place; otherwise it is inserted just before the closing fence.
fn insert_or_replace_frontmatter(text: &str, key: &str, value: &str) -> String {
    let key_prefix = format!("{key}:");
    let new_line = format!("{key}: {value}");
    let mut out = String::with_capacity(text.len() + new_line.len() + 1);
    let mut in_frontmatter = false;
    let mut seen_open = false;
    let mut replaced = false;
    let mut inserted = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if !seen_open {
                seen_open = true;
                in_frontmatter = true;
                out.push_str(line);
                out.push('\n');
                continue;
            } else if in_frontmatter {
                // Closing fence: insert the key here if we never saw it.
                if !replaced && !inserted {
                    out.push_str(&new_line);
                    out.push('\n');
                    inserted = true;
                }
                in_frontmatter = false;
                out.push_str(line);
                out.push('\n');
                continue;
            }
        }
        if in_frontmatter && trimmed.starts_with(&key_prefix) {
            out.push_str(&new_line);
            out.push('\n');
            replaced = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Count the live `.light.md` claims in a store dir (dot-dirs / `.history` excluded).
fn count_live_claims(store_dir: &Path) -> usize {
    std::fs::read_dir(store_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            !name.starts_with('.') && name.ends_with(".light.md") && e.path().is_file()
        })
        .count()
}

/// Shape a [`PromoteOutcome`] into the `promote` tool's JSON response.
pub fn promote_response(input: &PromoteInput, outcome: &PromoteOutcome) -> Value {
    let over_cap = outcome.medulla_claim_count > outcome.soft_cap;
    let mut resp = json!({
        "ok": true,
        "schema": "m1nd-promote-v0",
        "promoted": true,
        "medulla_path": outcome.medulla_path.to_string_lossy(),
        "medulla_slug": outcome.medulla_slug,
        "witness_path": outcome.witness_path.to_string_lossy(),
        "origin_brain": outcome.origin_brain,
        "promoted_by": input.agent_id,
        "promotion_reason": input.reason,
        // C8.2 evidence integrity — WHICH channel this promotion took.
        "evidence": {
            "origin_qualified": outcome.origin_qualified,
            "evidence_unverifiable": outcome.evidence_unverifiable,
            "note": if outcome.origin_qualified {
                "evidence paths were origin-qualified (<origin_root>#<path>) — freshness delegates to the origin brain (C8.2 channel a)"
            } else if outcome.evidence_unverifiable {
                "the claim is stamped evidence_unverifiable — no resolvable origin root to qualify against; it never reads fresher than it can prove (C8.2 channel b)"
            } else {
                "the claim carried no code evidence — declared tissue, nothing to re-anchor"
            },
        },
        "medulla_claim_count": outcome.medulla_claim_count,
        "soft_cap": outcome.soft_cap,
        "note": "promotion elevates, never moves: the project witness stays in place stamped Promoted-To; the medulla copy carries the full origin chain (Origin-Brain, Origin-Claim, Promoted-By, Promotion-Reason). Demotion (learn wrong / consolidation on the medulla copy) un-shares and never touches the witness.",
    });
    if over_cap {
        resp["cap_warning"] = json!(format!(
            "the medulla now holds {} claims — over the soft cap of {} (TT §6): run the consolidation pass to merge/supersede before it drifts from an index into a warehouse.",
            outcome.medulla_claim_count, outcome.soft_cap
        ));
    }
    resp
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(label: &str, evidence: Vec<&str>) -> LightClaim {
        LightClaim {
            label: label.into(),
            text: Some(format!("{label} claim body.")),
            kind: Some("entity".into()),
            confidence: Some("0.9".into()),
            ambiguity: None,
            evidence: evidence.into_iter().map(String::from).collect(),
            depends_on: vec![],
        }
    }

    // --- C8.3 gate ---

    #[test]
    fn c83_gate_allows_verified() {
        let fm = ClaimFrontmatter {
            state: Some("verified".into()),
            ..Default::default()
        };
        assert!(evidence_class_gate(&fm).is_ok());
    }

    #[test]
    fn c83_gate_allows_founder_sourced() {
        let fm = ClaimFrontmatter {
            state: Some("authored".into()),
            source_agent: Some("human:maintainer".into()),
            ..Default::default()
        };
        assert!(evidence_class_gate(&fm).is_ok());
    }

    #[test]
    fn c83_gate_refuses_unverified_maker_claim() {
        let fm = ClaimFrontmatter {
            state: Some("authored".into()),
            source_agent: Some("codex:maker".into()),
            ..Default::default()
        };
        let err = evidence_class_gate(&fm).unwrap_err();
        assert!(err.contains("C8.3"), "gate reason must cite C8.3: {err}");
    }

    // --- hygiene floor ---

    #[test]
    fn hygiene_floor_passes_clean_claim() {
        assert!(hygiene_floor("A perfectly clean doctrine claim about routing.").is_ok());
    }

    #[test]
    fn hygiene_floor_refuses_secret() {
        assert!(hygiene_floor("token: ghp_ABCDEFGHIJKLMNOP see auth").is_err());
        assert!(hygiene_floor("-----BEGIN RSA PRIVATE KEY-----").is_err());
    }

    #[test]
    fn hygiene_floor_refuses_conflict_marker() {
        assert!(hygiene_floor("line\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> other").is_err());
    }

    // --- C8.2 re-anchor ---

    #[test]
    fn reanchor_origin_qualifies_code_evidence() {
        let claims = vec![claim("Router", vec!["src/router.rs", "src/lib.rs"])];
        let r = reanchor_evidence(&claims, Some("/path/to/repo"));
        assert!(r.origin_qualified, "code evidence must origin-qualify");
        assert!(!r.evidence_unverifiable);
        assert_eq!(r.claims[0].evidence[0], "/path/to/repo#src/router.rs");
        assert_eq!(r.claims[0].evidence[1], "/path/to/repo#src/lib.rs");
    }

    #[test]
    fn reanchor_marks_unverifiable_without_origin() {
        let claims = vec![claim("Router", vec!["src/router.rs"])];
        // No resolvable origin root (medulla / empty) → unverifiable.
        let r = reanchor_evidence(&claims, Some("medulla"));
        assert!(!r.origin_qualified);
        assert!(
            r.evidence_unverifiable,
            "evidence with no origin root must be marked unverifiable"
        );
        // The raw path is carried as context, not silently dropped.
        assert_eq!(r.claims[0].evidence[0], "src/router.rs");
    }

    #[test]
    fn reanchor_no_evidence_is_neither() {
        let claims = vec![claim("Doctrine", vec![])];
        let r = reanchor_evidence(&claims, Some("/path/to/repo"));
        assert!(!r.origin_qualified);
        assert!(
            !r.evidence_unverifiable,
            "a claim with no evidence was never verified tissue — nothing to re-anchor"
        );
    }

    #[test]
    fn reanchor_is_idempotent_for_already_qualified() {
        let mut c = claim("Router", vec![]);
        c.evidence = vec!["/path/to/repo#src/router.rs".into()];
        let r = reanchor_evidence(&[c], Some("/path/to/repo"));
        assert_eq!(
            r.claims[0].evidence[0], "/path/to/repo#src/router.rs",
            "already-qualified evidence must not be double-prefixed"
        );
    }

    // --- parse round-trip ---

    #[test]
    fn parse_reads_frontmatter_and_claims() {
        let doc = "---\nProtocol: L1GHT/1.0\nNode: Router\nState: verified\nSource-Agent: codex:maker\nOrigin-Brain: /path/to/repo\n---\n\n# Router\n\n## Routing\n\nThe router dispatches by caller root.\n\n[⍂ entity: Router]\n[𝔻 confidence: 0.9]\n[𝔻 evidence: src/router.rs]\n";
        let p = parse_light_claim(doc);
        assert_eq!(p.frontmatter.node.as_deref(), Some("Router"));
        assert_eq!(p.frontmatter.state.as_deref(), Some("verified"));
        assert_eq!(p.frontmatter.source_agent.as_deref(), Some("codex:maker"));
        assert_eq!(p.frontmatter.origin_brain.as_deref(), Some("/path/to/repo"));
        assert_eq!(p.claims.len(), 1);
        assert_eq!(p.claims[0].label, "Router");
        assert_eq!(p.claims[0].confidence.as_deref(), Some("0.9"));
        assert_eq!(p.claims[0].evidence, vec!["src/router.rs".to_string()]);
        assert_eq!(
            p.claims[0].text.as_deref(),
            Some("The router dispatches by caller root.")
        );
    }

    // --- frontmatter insert ---

    #[test]
    fn insert_frontmatter_adds_before_closing_fence() {
        let doc = "---\nNode: X\nState: verified\n---\n\n# X\n";
        let out = insert_or_replace_frontmatter(doc, "Promoted-To", "medulla@x@123");
        assert!(out.contains("Promoted-To: medulla@x@123"));
        // Still a valid fence: the added line is inside the frontmatter.
        let fence_close = out.find("\n---\n\n# X").expect("closing fence intact");
        let stamp = out.find("Promoted-To").expect("stamp present");
        assert!(stamp < fence_close, "stamp must be inside the frontmatter");
    }

    #[test]
    fn insert_frontmatter_replaces_existing() {
        let doc = "---\nNode: X\nPromoted-To: old\n---\n";
        let out = insert_or_replace_frontmatter(doc, "Promoted-To", "new");
        assert!(out.contains("Promoted-To: new"));
        assert!(!out.contains("Promoted-To: old"));
    }
}
