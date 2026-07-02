// === m1nd-mcp/src/light_author_handlers.rs ===
//
// `memorize` tool — the first L1GHT *writer* in the m1nd stack.
// Everything else only parses .light.md; this handler generates them.

use crate::protocol::core::IngestInput;
use crate::session::SessionState;
use crate::util::now_ms;
use m1nd_core::error::{M1ndError, M1ndResult};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// serde default helpers
// ---------------------------------------------------------------------------

fn default_true() -> bool {
    true
}

fn default_merge() -> String {
    "merge".to_string()
}

// ---------------------------------------------------------------------------
// Input structs
// ---------------------------------------------------------------------------

/// A single knowledge claim to be written as a L1GHT marker block.
#[derive(Debug, Deserialize)]
pub struct LightClaim {
    /// Entity name → `[⍂ entity: <label>]` (or state/event glyph).
    pub label: String,
    /// Prose line rendered above the marker block (defaults to label).
    #[serde(default)]
    pub text: Option<String>,
    /// "entity" | "state" | "event" — controls the glyph used.
    #[serde(default)]
    pub kind: Option<String>,
    /// Confidence value or word ("0.7", "high", "medium", ...).
    #[serde(default)]
    pub confidence: Option<String>,
    /// Ambiguity descriptor.
    #[serde(default)]
    pub ambiguity: Option<String>,
    /// Repo-relative code paths that serve as evidence (one `[𝔻 evidence:]` per entry).
    #[serde(default)]
    pub evidence: Vec<String>,
    /// Dependency labels (one `[⟁ depends_on:]` per entry).
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// Input for the `memorize` MCP tool.
#[derive(Debug, Deserialize)]
pub struct LightAuthorInput {
    pub agent_id: String,
    /// Written as the `Node:` frontmatter header and the `# <node_label>` title.
    pub node_label: String,
    /// `## <title>` section heading (defaults to node_label).
    #[serde(default)]
    pub title: Option<String>,
    /// `State:` frontmatter value (default "authored").
    #[serde(default)]
    pub state: Option<String>,
    pub claims: Vec<LightClaim>,
    /// Override output path; default `<runtime_root>/agent-memory/<slug>.light.md`.
    #[serde(default)]
    pub output_path: Option<String>,
    /// Graph namespace passed to ingest (default "light").
    #[serde(default)]
    pub namespace: Option<String>,
    /// Whether to run ingest after writing (default true).
    #[serde(default = "default_true")]
    pub ingest_after: bool,
    /// Ingest merge mode (default "merge").
    #[serde(default = "default_merge")]
    pub mode: String,
    /// Internal only (never from the tool call): set to the slug this write
    /// supersedes, so a `Supersedes:` frontmatter line is rendered. `#[serde(skip)]`
    /// keeps it off the public tool schema — the handler fills it during the
    /// invalidate-and-keep sequence.
    #[serde(skip)]
    pub supersedes: Option<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Handle the `memorize` MCP tool call.
pub fn handle_light_author(
    state: &mut SessionState,
    mut input: LightAuthorInput,
) -> M1ndResult<Value> {
    // 1. Resolve output path.
    let out_path = resolve_output_path(state, &input)?;

    // 2. Write to disk under supersession-on-rewrite (invalidate-and-keep) for the
    //    default agent-memory path; an explicit `output_path` override keeps the
    //    historical unconditional-write behavior (no lock, no supersession).
    let is_default_path = input.output_path.is_none();

    let parent = out_path.parent().ok_or_else(|| M1ndError::InvalidParams {
        tool: "memorize".into(),
        detail: "output path has no parent directory".into(),
    })?;
    fs::create_dir_all(parent).map_err(M1ndError::Io)?;

    let (markdown, supersession) = if is_default_path {
        // Per-slug exclusive lock held across the whole read-modify-write, dropped
        // BEFORE ingest (ingest only reads). This is what makes two sibling sessions
        // on the same slug safe under multi-session drift.
        let slug = slugify(&input.node_label);
        let _lock = LockGuard::acquire(&state.runtime_root, &slug)?;

        match plan_supersession(&out_path, &input)? {
            SupersessionPlan::WouldDowngrade { reason } => {
                // The stronger prior stays live; we refuse the weaker write rather
                // than silently dropping it — the agent is told why.
                let path_str = out_path.to_string_lossy().to_string();
                return Ok(json!({
                    "ok": true,
                    "schema": "m1nd-memorize-v0",
                    "path": path_str,
                    "bytes_written": 0,
                    "claims_written": 0,
                    "ingested": false,
                    "superseded": false,
                    "reason": reason,
                    "note": "weaker write refused: the stronger prior memory is kept live (invalidate-and-keep gate).",
                }));
            }
            SupersessionPlan::Supersede => {
                // Retain the prior belief in .history/ flipped to `State: outdated`,
                // then stamp the new file with its Supersedes lineage.
                archive_prior_as_outdated(&out_path, &slug, &state.runtime_root)?;
                input.supersedes = Some(slug.clone());
                let md = render_light_markdown(&input);
                write_atomic(&out_path, &md)?;
                (md, Some(true))
            }
            SupersessionPlan::FirstWrite => {
                let md = render_light_markdown(&input);
                write_atomic(&out_path, &md)?;
                (md, None)
            }
        }
        // _lock dropped here — before ingest.
    } else {
        // Explicit override path: unchanged historical behavior.
        let md = render_light_markdown(&input);
        fs::write(&out_path, &md).map_err(M1ndError::Io)?;
        (md, None)
    };

    let bytes_written = markdown.len();
    let claims_written = input.claims.len();
    let path_str = out_path.to_string_lossy().to_string();

    // 4. Optionally ingest.
    if input.ingest_after {
        let ingest_input = IngestInput {
            path: path_str.clone(),
            agent_id: input.agent_id.clone(),
            incremental: false,
            adapter: "light".into(),
            mode: input.mode.clone(),
            namespace: Some(input.namespace.clone().unwrap_or_else(|| "light".into())),
            include_dotfiles: false,
            dotfile_patterns: vec![],
        };
        let ingest_result = crate::tools::handle_ingest(state, ingest_input)?;

        let node_count = ingest_result["node_count"].as_u64().unwrap_or(0);
        let edge_count = ingest_result["edge_count"].as_u64().unwrap_or(0);
        let resolved = ingest_result["light_evidence_resolved"]
            .as_u64()
            .unwrap_or(0);
        let unresolved = ingest_result["light_evidence_unresolved"]
            .as_u64()
            .unwrap_or(0);

        // Only-when-relevant guidance: unresolved evidence usually means the cited
        // code was not ingested, or the path is not repo-relative to the code root.
        let next_action = if unresolved > 0 {
            format!(
                "{} evidence path(s) did not resolve to a code node — ingest the code first (ingest adapter=code) and ensure evidence paths are repo-relative to that root, then re-run memorize so the knowledge anchors and cross_verify(check:[\"evidence_freshness\"]) can track it.",
                unresolved
            )
        } else if resolved > 0 {
            "Memory anchored to code and will auto-load next session; cross_verify(check:[\"evidence_freshness\"]) flags it if the cited code changes.".to_string()
        } else {
            "Memory persisted and will auto-load next session. Add `evidence` paths to claims to anchor them to code and enable staleness detection.".to_string()
        };

        let mut resp = json!({
            "ok": true,
            "schema": "m1nd-memorize-v0",
            "path": path_str,
            "bytes_written": bytes_written,
            "claims_written": claims_written,
            "ingested": true,
            "node_count": node_count,
            "edge_count": edge_count,
            "light_evidence_resolved": resolved,
            "light_evidence_unresolved": unresolved,
            "next_action": next_action,
            "rendered": markdown,
        });
        if let Some(superseded) = supersession {
            resp["superseded"] = json!(superseded);
        }
        return Ok(resp);
    }

    let mut resp = json!({
        "ok": true,
        "schema": "m1nd-memorize-v0",
        "path": path_str,
        "bytes_written": bytes_written,
        "claims_written": claims_written,
        "ingested": false,
        "rendered": markdown,
    });
    if let Some(superseded) = supersession {
        resp["superseded"] = json!(superseded);
    }
    Ok(resp)
}

// ---------------------------------------------------------------------------
// Rendering (the new L1GHT writer)
// ---------------------------------------------------------------------------

/// Render a valid `.light.md` document from the given input.
///
/// The entity/state/event marker (`[⍂ entity: ...]`) is emitted BEFORE the
/// epistemic `[𝔻 ...]` qualifiers for each claim.  This is critical because
/// the parser's `last_claim_id` attaches 𝔻 qualifiers to the most-recent
/// non-epistemic claim; reversing the order would attach them to the wrong node.
pub fn render_light_markdown(input: &LightAuthorInput) -> String {
    let state_val = input.state.as_deref().unwrap_or("authored");
    let title_val = input.title.as_deref().unwrap_or(input.node_label.as_str());

    let mut out = String::new();

    // Frontmatter
    out.push_str("---\n");
    out.push_str("Protocol: L1GHT/1.0\n");
    out.push_str(&format!("Node: {}\n", input.node_label));
    out.push_str(&format!("State: {}\n", state_val));
    // Provenance: when this memory was written and which agent authored it.
    // Missing on older `.light.md` files is honestly "unknown" — the parser
    // ignores unknown frontmatter keys, so these are backward-compatible.
    out.push_str(&format!("Created: {}\n", now_ms()));
    out.push_str(&format!("Source-Agent: {}\n", input.agent_id));
    // Supersession lineage: names the slug whose prior belief this write invalidates
    // (the prior copy is retained in `agent-memory/.history/` as `State: outdated`).
    // Frontmatter-only for now; the parser tolerates unknown keys. A graph-visible
    // supersedes edge is deferred.
    if let Some(superseded) = &input.supersedes {
        out.push_str(&format!("Supersedes: {}\n", superseded));
    }
    out.push_str("---\n");
    out.push('\n');

    // Title
    out.push_str(&format!("# {}\n", input.node_label));
    out.push('\n');

    // Section heading
    out.push_str(&format!("## {}\n", title_val));
    out.push('\n');

    // Claims
    for claim in &input.claims {
        // Prose line (defaults to label)
        let prose = claim.text.as_deref().unwrap_or(claim.label.as_str());
        out.push_str(prose);
        out.push('\n');
        out.push('\n');

        // Entity/state/event marker FIRST (so 𝔻 qualifiers attach to it)
        let (glyph, kind_word) = claim_glyph(claim.kind.as_deref());
        out.push_str(&format!("[{} {}: {}]\n", glyph, kind_word, claim.label));

        // Epistemic qualifiers (attach to the preceding non-epistemic marker)
        if let Some(conf) = &claim.confidence {
            out.push_str(&format!("[𝔻 confidence: {}]\n", conf));
        }
        if let Some(amb) = &claim.ambiguity {
            out.push_str(&format!("[𝔻 ambiguity: {}]\n", amb));
        }
        for ev in &claim.evidence {
            out.push_str(&format!("[𝔻 evidence: {}]\n", ev));
        }
        for dep in &claim.depends_on {
            out.push_str(&format!("[⟁ depends_on: {}]\n", dep));
        }

        out.push('\n');
    }

    out
}

/// Return `(glyph, kind_word)` for a claim kind string.
/// - "entity" → (⍂, "entity")
/// - "state"  → (⍐, "state")
/// - "event"  → (⍌, "event")
/// - anything else / None → (⍂, "entity")
fn claim_glyph(kind: Option<&str>) -> (&'static str, &'static str) {
    match kind {
        Some("state") => ("⍐", "state"),
        Some("event") => ("⍌", "event"),
        _ => ("⍂", "entity"),
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn resolve_output_path(state: &SessionState, input: &LightAuthorInput) -> M1ndResult<PathBuf> {
    if let Some(ref override_path) = input.output_path {
        return Ok(PathBuf::from(override_path));
    }
    let slug = slugify(&input.node_label);
    let filename = format!("{}.light.md", slug);
    Ok(state.runtime_root.join("agent-memory").join(filename))
}

/// Lowercase alnum, non-alnum → '-', collapse consecutive '-'.
fn slugify(s: &str) -> String {
    let mut result = String::new();
    let mut last_was_dash = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            result.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            result.push('-');
            last_was_dash = true;
        }
    }
    // Trim trailing dash
    result.trim_end_matches('-').to_string()
}

// ---------------------------------------------------------------------------
// Supersession-on-rewrite (invalidate-and-keep)
// ---------------------------------------------------------------------------

/// Per-slug advisory file lock held across the read-modify-write of one memory.
///
/// RAII mirror of `instance_registry::InstanceHandle`: acquire on construction,
/// release on `Drop`. Backed by `libc::flock(LOCK_EX)` on a `.locks/<slug>.lock`
/// file — a blocking, whole-file exclusive lock that is per-open-file-description,
/// so two sibling sessions (or two threads with independent `open`s) serialize
/// correctly. Blocking (not try-lock) is deliberate: memorize is durable and
/// low-frequency, so correctness beats latency.
///
/// On non-unix targets there is no `flock`; the guard is a documented no-op and
/// supersession still runs (single-writer assumption — the only degradation is
/// the loss of cross-process serialization, which unix always has).
struct LockGuard {
    #[cfg(unix)]
    fd: std::os::unix::io::RawFd,
}

impl LockGuard {
    fn acquire(runtime_root: &Path, slug: &str) -> M1ndResult<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let locks_dir = runtime_root.join("agent-memory").join(".locks");
            fs::create_dir_all(&locks_dir).map_err(M1ndError::Io)?;
            let lock_path = locks_dir.join(format!("{}.lock", slug));
            let file = fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(&lock_path)
                .map_err(M1ndError::Io)?;
            let fd = file.as_raw_fd();
            // Blocking exclusive lock. `file` is leaked (into_raw handled below) so
            // the fd stays open until we release+close in Drop.
            // SAFETY: `fd` is a valid open descriptor for the lifetime of this call.
            let rc = unsafe { libc::flock(fd, libc::LOCK_EX) };
            if rc != 0 {
                return Err(M1ndError::Io(std::io::Error::last_os_error()));
            }
            // Keep the fd alive past `file`'s scope; Drop closes it.
            let raw = {
                use std::os::unix::io::IntoRawFd;
                file.into_raw_fd()
            };
            Ok(LockGuard { fd: raw })
        }
        #[cfg(not(unix))]
        {
            let _ = (runtime_root, slug);
            Ok(LockGuard {})
        }
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            // Release the lock, then close the descriptor. Errors on teardown are
            // non-actionable (best-effort), matching how the OS reclaims on close.
            // SAFETY: `self.fd` is the descriptor we opened and locked in `acquire`.
            unsafe {
                libc::flock(self.fd, libc::LOCK_UN);
                libc::close(self.fd);
            }
        }
    }
}

/// What the invalidate-and-keep gate decided for a write to `out_path`.
enum SupersessionPlan {
    /// No prior file — a plain first write.
    FirstWrite,
    /// A prior file exists and the new claim is at least as strong — supersede it.
    Supersede,
    /// A prior file exists and is strictly stronger — refuse the weaker write.
    WouldDowngrade { reason: String },
}

/// A prior memory's epistemic strength, parsed from its frontmatter/markers.
struct PriorStrength {
    state_rank: u8,
    /// `None` when no confidence could be parsed (fail-safe: unknown).
    confidence: Option<f32>,
}

/// State ordering: verified (2) > authored (1) > outdated (0). Unknown ⇒ None.
fn state_rank(state: &str) -> Option<u8> {
    match state.trim().to_ascii_lowercase().as_str() {
        "verified" => Some(2),
        "authored" => Some(1),
        "outdated" => Some(0),
        _ => None,
    }
}

/// Confidence scalar. Numeric (`0.9`) parsed directly; words high/medium/low
/// mapped to 0.9/0.6/0.3. Anything else ⇒ `None` (unknown — fail-safe).
fn confidence_scalar(raw: &str) -> Option<f32> {
    let t = raw.trim().trim_end_matches(['.', ',', ';']);
    if let Ok(v) = t.parse::<f32>() {
        return Some(v);
    }
    match t.to_ascii_lowercase().as_str() {
        "high" => Some(0.9),
        "medium" => Some(0.6),
        "low" => Some(0.3),
        _ => None,
    }
}

/// Scan a `.light.md`'s header for `State:` and the max `[𝔻 confidence: …]` claim.
/// A tiny local scan rather than reaching across the crate boundary into
/// `l1ght_adapter::parse_header` (private, different crate) — the spec's fallback.
fn scan_prior_strength(text: &str) -> PriorStrength {
    let mut state = "authored".to_string();
    let mut max_conf: Option<f32> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(v) = trimmed.strip_prefix("State:") {
            state = v.trim().to_string();
        } else if let Some(rest) = trimmed.strip_prefix("[𝔻 confidence:") {
            let val = rest.trim_end_matches(']').trim();
            if let Some(c) = confidence_scalar(val) {
                max_conf = Some(max_conf.map_or(c, |m| m.max(c)));
            }
        }
    }
    PriorStrength {
        state_rank: state_rank(&state).unwrap_or(0),
        confidence: max_conf,
    }
}

/// Strength of the NEW write, from its input (before render).
fn new_strength(input: &LightAuthorInput) -> PriorStrength {
    let state = input.state.as_deref().unwrap_or("authored");
    let mut max_conf: Option<f32> = None;
    for claim in &input.claims {
        if let Some(conf) = &claim.confidence {
            if let Some(c) = confidence_scalar(conf) {
                max_conf = Some(max_conf.map_or(c, |m| m.max(c)));
            }
        }
    }
    PriorStrength {
        state_rank: state_rank(state).unwrap_or(0),
        confidence: max_conf,
    }
}

/// Decide the invalidate-and-keep plan for a write to `out_path`.
///
/// - No prior file ⇒ `FirstWrite`.
/// - Prior exists ⇒ read its strength and run the gate against the new write.
fn plan_supersession(out_path: &Path, input: &LightAuthorInput) -> M1ndResult<SupersessionPlan> {
    if !out_path.exists() {
        return Ok(SupersessionPlan::FirstWrite);
    }
    let prior_text = fs::read_to_string(out_path).map_err(M1ndError::Io)?;
    let prior = scan_prior_strength(&prior_text);
    let new = new_strength(input);
    Ok(gate_supersession(&prior, &new))
}

/// The gate: "weaker can't clobber stronger." Supersede only if the new write is
/// at least as strong on BOTH axes. Fail-safe: if either side's confidence is
/// unknown/unparseable (so the comparison can't be made confidently), do NOT
/// supersede — keep the stronger prior live.
fn gate_supersession(prior: &PriorStrength, new: &PriorStrength) -> SupersessionPlan {
    let (Some(new_conf), Some(prior_conf)) = (new.confidence, prior.confidence) else {
        // Unknown confidence on either side ⇒ can't confidently compare ⇒ refuse.
        return SupersessionPlan::WouldDowngrade {
            reason: "would_downgrade".to_string(),
        };
    };
    if new.state_rank >= prior.state_rank && new_conf >= prior_conf {
        SupersessionPlan::Supersede
    } else {
        SupersessionPlan::WouldDowngrade {
            reason: "would_downgrade".to_string(),
        }
    }
}

/// Copy the live prior file into `.history/<slug>.<ts>.light.md` with its `State:`
/// flipped to `outdated` — retained forever as the audit trail. The live file is
/// left untouched here; the caller overwrites it with the new claim afterward.
fn archive_prior_as_outdated(out_path: &Path, slug: &str, runtime_root: &Path) -> M1ndResult<()> {
    let prior_text = fs::read_to_string(out_path).map_err(M1ndError::Io)?;
    let outdated = flip_state_to_outdated(&prior_text);
    let history_dir = runtime_root.join("agent-memory").join(".history");
    fs::create_dir_all(&history_dir).map_err(M1ndError::Io)?;
    let history_path = history_dir.join(format!("{}.{}.light.md", slug, now_ms()));
    write_atomic(&history_path, &outdated)?;
    Ok(())
}

/// Return `text` with the first `State:` frontmatter line rewritten to
/// `State: outdated` (idempotent if already outdated).
fn flip_state_to_outdated(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut flipped = false;
    for line in text.lines() {
        if !flipped && line.trim_start().starts_with("State:") {
            out.push_str("State: outdated");
            flipped = true;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// Atomic write: temp file beside the target + rename (FM-PL-008), so a reader
/// (or a crashed writer) never sees a torn `.light.md`.
fn write_atomic(path: &Path, contents: &str) -> M1ndResult<()> {
    let parent = path.parent().ok_or_else(|| M1ndError::InvalidParams {
        tool: "memorize".into(),
        detail: "atomic write target has no parent directory".into(),
    })?;
    fs::create_dir_all(parent).map_err(M1ndError::Io)?;
    // Unique-ish temp name in the same dir (same filesystem ⇒ rename is atomic).
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "memory.light.md".to_string());
    let temp_path = parent.join(format!(".{}.{}.tmp", file_name, now_ms()));
    fs::write(&temp_path, contents).map_err(M1ndError::Io)?;
    fs::rename(&temp_path, path).map_err(M1ndError::Io)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::McpConfig;
    use crate::session::SessionState;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;
    use m1nd_core::types::NodeType;

    fn make_input(claims: Vec<LightClaim>) -> LightAuthorInput {
        LightAuthorInput {
            agent_id: "test-agent".into(),
            node_label: "AuthSystem".into(),
            title: Some("Authentication System".into()),
            state: Some("verified".into()),
            claims,
            output_path: None,
            namespace: None,
            ingest_after: false,
            mode: "merge".into(),
            supersedes: None,
        }
    }

    fn build_session(root: &std::path::Path) -> SessionState {
        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };
        SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("init session")
    }

    // -----------------------------------------------------------------------
    // Test 1: render produces valid L1GHT structure
    // -----------------------------------------------------------------------
    #[test]
    fn memorize_renders_valid_l1ght() {
        let input = make_input(vec![
            LightClaim {
                label: "TokenValidator".into(),
                text: Some("The token validator checks JWT signatures.".into()),
                kind: Some("entity".into()),
                confidence: Some("0.9".into()),
                ambiguity: None,
                evidence: vec!["auth.rs".into()],
                depends_on: vec!["JwtLibrary".into()],
            },
            LightClaim {
                label: "SessionExpiry".into(),
                text: None,
                kind: Some("state".into()),
                confidence: None,
                ambiguity: None,
                evidence: vec![],
                depends_on: vec![],
            },
        ]);

        let md = render_light_markdown(&input);

        // Frontmatter present
        assert!(
            md.contains("Protocol: L1GHT/1.0"),
            "missing protocol header"
        );
        assert!(md.contains("Node: AuthSystem"), "missing Node header");

        // Entity marker is before 𝔻 confidence
        let entity_pos = md
            .find("[⍂ entity: TokenValidator]")
            .expect("entity marker missing");
        let conf_pos = md
            .find("[𝔻 confidence: 0.9]")
            .expect("confidence marker missing");
        assert!(
            entity_pos < conf_pos,
            "entity marker must appear before 𝔻 confidence marker (parser attaches 𝔻 to last non-epistemic claim)"
        );

        // Evidence marker present
        assert!(
            md.contains("[𝔻 evidence: auth.rs]"),
            "evidence marker missing"
        );

        // State glyph used for SessionExpiry
        assert!(
            md.contains("[⍐ state: SessionExpiry]"),
            "state glyph missing"
        );
    }

    // -----------------------------------------------------------------------
    // Test 2: writes file and ingests, resolving evidence bridge
    // -----------------------------------------------------------------------
    #[test]
    fn memorize_writes_and_ingests_with_evidence_bridge() {
        let temp = tempfile::tempdir().expect("tempdir");
        let proj = temp.path().join("proj");
        std::fs::create_dir_all(&proj).expect("proj dir");

        // Write a real code file so the code node `file::auth.rs` exists after ingest.
        std::fs::write(
            proj.join("auth.rs"),
            "pub fn validate_token(t: &str) -> bool { !t.is_empty() }\n",
        )
        .expect("write auth.rs");

        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };

        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");

        // 1. Ingest the code so `file::auth.rs` exists in graph.
        let code_ingest = IngestInput {
            path: proj.to_string_lossy().to_string(),
            agent_id: "test".into(),
            incremental: false,
            adapter: "code".into(),
            mode: "replace".into(),
            namespace: None,
            include_dotfiles: false,
            dotfile_patterns: vec![],
        };
        crate::tools::handle_ingest(&mut state, code_ingest).expect("code ingest");

        // 2. Call handle_light_author with evidence="auth.rs" and ingest_after=true.
        let input = LightAuthorInput {
            agent_id: "test".into(),
            node_label: "AuthNotes".into(),
            title: None,
            state: None,
            claims: vec![LightClaim {
                label: "TokenValidator".into(),
                text: Some("The token validator checks JWT signatures.".into()),
                kind: Some("entity".into()),
                confidence: Some("0.9".into()),
                ambiguity: None,
                evidence: vec!["auth.rs".into()],
                depends_on: vec![],
            }],
            output_path: None,
            namespace: None,
            ingest_after: true,
            mode: "merge".into(),
            supersedes: None,
        };

        let result = handle_light_author(&mut state, input).expect("memorize ok");

        // File must exist on disk.
        let path_str = result["path"].as_str().expect("path field");
        assert!(
            std::path::Path::new(path_str).exists(),
            "output file not created: {}",
            path_str
        );

        // Evidence must have resolved (≥1).
        let resolved = result["light_evidence_resolved"].as_u64().unwrap_or(0);
        assert!(
            resolved >= 1,
            "expected >=1 light_evidence_resolved, got {}",
            resolved
        );

        // Result shape.
        assert_eq!(result["ok"], true);
        assert_eq!(result["ingested"], true);
        assert_eq!(result["schema"], "m1nd-memorize-v0");
    }

    // -----------------------------------------------------------------------
    // Test 3: provenance frontmatter (Created + Source-Agent) is stamped
    // -----------------------------------------------------------------------
    #[test]
    fn memorize_stamps_created_and_source_agent() {
        let before = now_ms();

        let input = make_input(vec![LightClaim {
            label: "TokenValidator".into(),
            text: None,
            kind: Some("entity".into()),
            confidence: None,
            ambiguity: None,
            evidence: vec![],
            depends_on: vec![],
        }]);

        let md = render_light_markdown(&input);
        let after = now_ms();

        // Source-Agent equals the input agent_id ("test-agent" from make_input).
        assert!(
            md.contains("Source-Agent: test-agent"),
            "Source-Agent frontmatter missing or wrong, got:\n{}",
            md
        );

        // Created is present with a plausible unix-millis value inside [before, after].
        let created_line = md
            .lines()
            .find_map(|l| l.strip_prefix("Created: "))
            .expect("Created frontmatter line missing");
        let created: u64 = created_line
            .trim()
            .parse()
            .expect("Created value is not unix millis");
        assert!(
            created >= before && created <= after,
            "Created={} not within [{}, {}] — implausible timestamp",
            created,
            before,
            after
        );

        // Provenance lives in frontmatter (before the closing `---`/title).
        let created_pos = md.find("Created:").expect("Created pos");
        let title_pos = md.find("# AuthSystem").expect("title pos");
        assert!(
            created_pos < title_pos,
            "Created must be in frontmatter, before the title"
        );
    }

    // -----------------------------------------------------------------------
    // Test 4: backward compat — a legacy .light.md lacking the new fields
    // still ingests cleanly (missing = unknown, never an error).
    // -----------------------------------------------------------------------
    #[test]
    fn legacy_light_md_without_provenance_still_ingests() {
        let temp = tempfile::tempdir().expect("tempdir");
        let proj = temp.path().join("proj");
        std::fs::create_dir_all(&proj).expect("proj dir");

        // Hand-written legacy memory: only the pre-provenance frontmatter keys.
        let legacy = "---\nProtocol: L1GHT/1.0\nNode: LegacyNode\nState: authored\n---\n\n# LegacyNode\n\n## LegacyNode\n\nA legacy claim with no provenance.\n\n[⍂ entity: LegacyClaim]\n[𝔻 confidence: 0.8]\n";
        std::fs::write(proj.join("legacy.light.md"), legacy).expect("write legacy");

        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");

        let ingest = IngestInput {
            path: proj.to_string_lossy().to_string(),
            agent_id: "test".into(),
            incremental: false,
            adapter: "light".into(),
            mode: "merge".into(),
            namespace: Some("light".into()),
            include_dotfiles: false,
            dotfile_patterns: vec![],
        };

        // Must NOT error on the absent Created/Source-Agent keys.
        let result = crate::tools::handle_ingest(&mut state, ingest)
            .expect("legacy light .md must ingest without error");

        let node_count = result["node_count"].as_u64().unwrap_or(0);
        assert!(
            node_count >= 1,
            "legacy light .md should still produce nodes, got node_count={}",
            node_count
        );
    }

    // -----------------------------------------------------------------------
    // Test 5: slugify helper
    // -----------------------------------------------------------------------
    #[test]
    fn slugify_lowercases_and_replaces_non_alnum() {
        assert_eq!(slugify("AuthSystem"), "authsystem");
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("foo::bar::baz"), "foo-bar-baz");
        assert_eq!(slugify("  leading"), "-leading");
    }

    // -----------------------------------------------------------------------
    // Supersession-on-rewrite tests
    // -----------------------------------------------------------------------

    /// Build a memorize input for the default (agent-memory) path, no ingest,
    /// with a single claim carrying the given confidence.
    fn super_input(node: &str, state: &str, confidence: &str) -> LightAuthorInput {
        LightAuthorInput {
            agent_id: "test-agent".into(),
            node_label: node.into(),
            title: None,
            state: Some(state.into()),
            claims: vec![LightClaim {
                label: "Claim".into(),
                text: Some("A claim.".into()),
                kind: Some("entity".into()),
                confidence: Some(confidence.into()),
                ambiguity: None,
                evidence: vec![],
                depends_on: vec![],
            }],
            output_path: None,
            namespace: None,
            ingest_after: false,
            mode: "merge".into(),
            supersedes: None,
        }
    }

    fn agent_memory_dir(state: &SessionState) -> PathBuf {
        state.runtime_root.join("agent-memory")
    }

    // Test: auto-supersede same slug — prior copied to .history as `State: outdated`,
    // new file carries `Supersedes:`, and nothing is deleted.
    #[test]
    fn supersession_auto_supersedes_same_slug() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_session(temp.path());

        // First write: authored, 0.6.
        handle_light_author(&mut state, super_input("X", "authored", "0.6"))
            .expect("first memorize");
        let live = agent_memory_dir(&state).join("x.light.md");
        assert!(live.exists(), "first write should create the live file");

        // Second write: verified, 0.9 — should supersede.
        let result = handle_light_author(&mut state, super_input("X", "verified", "0.9"))
            .expect("second memorize");
        assert_eq!(result["superseded"], true, "second write should supersede");

        // The live file is the NEW claim, stamped with Supersedes.
        let live_text = std::fs::read_to_string(&live).expect("read live");
        assert!(
            live_text.contains("Supersedes: x"),
            "new live file must carry Supersedes lineage, got:\n{}",
            live_text
        );
        assert!(
            live_text.contains("State: verified"),
            "new live file must be the verified claim"
        );

        // The prior is retained in .history/, flipped to outdated. Nothing deleted.
        let history_dir = agent_memory_dir(&state).join(".history");
        let entries: Vec<_> = std::fs::read_dir(&history_dir)
            .expect("history dir")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("x."))
            })
            .collect();
        assert_eq!(entries.len(), 1, "exactly one archived prior expected");
        let archived = std::fs::read_to_string(&entries[0]).expect("read archived");
        assert!(
            archived.contains("State: outdated"),
            "archived prior must be flipped to outdated, got:\n{}",
            archived
        );
        assert!(
            live.exists(),
            "live file must still exist (nothing deleted)"
        );
    }

    // Test: downgrade gate — a weaker write must NOT clobber a stronger prior.
    #[test]
    fn supersession_downgrade_gate_refuses_weaker_write() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_session(temp.path());

        // Prior: verified, 0.9.
        handle_light_author(&mut state, super_input("X", "verified", "0.9"))
            .expect("first memorize");
        let live = agent_memory_dir(&state).join("x.light.md");
        let before = std::fs::read_to_string(&live).expect("read live before");

        // New: authored, 0.5 — strictly weaker, must be refused.
        let result = handle_light_author(&mut state, super_input("X", "authored", "0.5"))
            .expect("second memorize");
        assert_eq!(result["superseded"], false, "weaker write must be refused");
        assert_eq!(result["reason"], "would_downgrade");

        // Live file UNCHANGED — still the verified prior.
        let after = std::fs::read_to_string(&live).expect("read live after");
        assert_eq!(
            before, after,
            "live file must be unchanged by the refused write"
        );
        assert!(
            after.contains("State: verified"),
            "prior must stay verified live"
        );

        // No .history copy was made for the refused write.
        let history_dir = agent_memory_dir(&state).join(".history");
        let history_count = std::fs::read_dir(&history_dir)
            .map(|d| d.filter_map(Result::ok).count())
            .unwrap_or(0);
        assert_eq!(history_count, 0, "no archive on a refused downgrade");
    }

    // Test: reload ignores history — a live memory + an outdated .history copy →
    // light ingest with include_dotfiles:false must NOT surface the .history claim.
    // Proof: ingesting the whole agent-memory dir yields the SAME node count as
    // ingesting the live file alone, because the `.history` dot-dir is pruned.
    #[test]
    fn supersession_reload_ignores_history() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = build_session(temp.path());

        // Create live + history via a real supersession.
        handle_light_author(&mut state, super_input("Widget", "authored", "0.6"))
            .expect("first memorize");
        handle_light_author(&mut state, super_input("Widget", "verified", "0.9"))
            .expect("supersede memorize");

        let mem_dir = agent_memory_dir(&state);
        assert!(
            mem_dir.join(".history").exists(),
            "history dir should exist"
        );
        assert!(
            mem_dir.join("widget.light.md").exists(),
            "live file should exist"
        );

        // Count nodes from the LIVE file alone (single-file ingest).
        let live_only = IngestInput {
            path: mem_dir
                .join("widget.light.md")
                .to_string_lossy()
                .to_string(),
            agent_id: "test".into(),
            incremental: false,
            adapter: "light".into(),
            mode: "replace".into(),
            namespace: Some("light".into()),
            include_dotfiles: false,
            dotfile_patterns: vec![],
        };
        let live_count = crate::tools::handle_ingest(&mut state, live_only).expect("live ingest")
            ["node_count"]
            .as_u64()
            .unwrap_or(0);
        assert!(live_count >= 1, "the live memory should ingest");

        // Ingest the whole dir the way the runtime does (dotfiles excluded).
        let dir_ingest = IngestInput {
            path: mem_dir.to_string_lossy().to_string(),
            agent_id: "test".into(),
            incremental: false,
            adapter: "light".into(),
            mode: "replace".into(),
            namespace: Some("light".into()),
            include_dotfiles: false,
            dotfile_patterns: vec![],
        };
        let dir_count = crate::tools::handle_ingest(&mut state, dir_ingest).expect("dir ingest")
            ["node_count"]
            .as_u64()
            .unwrap_or(0);

        // Same count ⇒ the `.history` copy contributed nothing (it was pruned).
        assert_eq!(
            dir_count, live_count,
            "whole-dir ingest must equal live-only ingest — the .history copy must be pruned, not reloaded"
        );
    }

    // Test: concurrency (the flock proof) — two threads memorize the same slug
    // against one runtime_root; afterward exactly ONE live file, no torn write,
    // no double-archive beyond the serialized supersession.
    #[test]
    fn supersession_concurrent_same_slug_is_serialized() {
        use std::sync::Arc;
        use std::thread;

        let temp = tempfile::tempdir().expect("tempdir");
        let root = Arc::new(temp.path().to_path_buf());

        // Seed a first version so both threads race to supersede the SAME prior.
        {
            let mut state = build_session(root.as_path());
            handle_light_author(&mut state, super_input("Race", "authored", "0.5")).expect("seed");
        }

        // Build BOTH sessions SEQUENTIALLY, before spawning any thread. Each is an
        // independent SessionState pointing at the SAME runtime_root — the real
        // multi-session-drift shape — but session INITIALIZATION is not what this
        // test proves and is not concurrent-safe (two same-pid ReadWrite acquirers
        // race the shared instance-lease write in instance_registry). Initializing
        // here, off the hot path, isolates the property under test: the per-slug
        // flock in handle_light_author is the ONLY serializer of the concurrent
        // read-modify-write, exactly as it is across two live sibling processes.
        let sessions: Vec<SessionState> =
            (0..2u32).map(|_| build_session(root.as_path())).collect();

        let mut handles = Vec::new();
        for (i, mut state) in sessions.into_iter().enumerate() {
            handles.push(thread::spawn(move || {
                // Race ONLY the memorize RMW; the flock must serialize it.
                let conf = if i == 0 { "0.9" } else { "0.8" };
                let _ = handle_light_author(&mut state, super_input("Race", "verified", conf));
            }));
        }
        for h in handles {
            h.join().expect("thread join");
        }

        // Exactly ONE live file, and it is a complete (non-torn) L1GHT document.
        // agent-memory lives under runtime_root (= <root>/runtime), matching
        // build_session's runtime_dir.
        let mem_dir = root.join("runtime").join("agent-memory");
        let live = mem_dir.join("race.light.md");
        assert!(live.exists(), "exactly one live file must remain");
        let text = std::fs::read_to_string(&live).expect("read live");
        assert!(
            text.contains("Protocol: L1GHT/1.0") && text.trim_end().ends_with("]"),
            "live file must be a complete, non-torn document, got:\n{}",
            text
        );
        // No leftover temp files (atomic rename cleaned up).
        let stray_temp = std::fs::read_dir(&mem_dir)
            .expect("read mem dir")
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().ends_with(".tmp"));
        assert!(!stray_temp, "no torn/leftover .tmp files should remain");
    }

    // Test: frontmatter round-trip — a rendered doc with Supersedes parses cleanly
    // (the ingest parser tolerates the unknown key, no error).
    #[test]
    fn supersession_supersedes_frontmatter_round_trips() {
        let mut input = super_input("Round", "verified", "0.9");
        input.supersedes = Some("round".into());
        let md = render_light_markdown(&input);
        assert!(
            md.contains("Supersedes: round"),
            "Supersedes line must render, got:\n{}",
            md
        );

        // Ingest it: the parser must not error on the unknown Supersedes key.
        let temp = tempfile::tempdir().expect("tempdir");
        let proj = temp.path().join("proj");
        std::fs::create_dir_all(&proj).expect("proj dir");
        std::fs::write(proj.join("round.light.md"), &md).expect("write");

        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");
        let ingest = IngestInput {
            path: proj.to_string_lossy().to_string(),
            agent_id: "test".into(),
            incremental: false,
            adapter: "light".into(),
            mode: "merge".into(),
            namespace: Some("light".into()),
            include_dotfiles: false,
            dotfile_patterns: vec![],
        };
        let result = crate::tools::handle_ingest(&mut state, ingest)
            .expect("doc with Supersedes must ingest without error");
        assert!(result["node_count"].as_u64().unwrap_or(0) >= 1);
    }
}
