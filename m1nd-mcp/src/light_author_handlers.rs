// === m1nd-mcp/src/light_author_handlers.rs ===
//
// `memorize` tool — the first L1GHT *writer* in the m1nd stack.
// Everything else only parses .light.md; this handler generates them.

use crate::protocol::core::IngestInput;
use crate::session::SessionState;
use m1nd_core::error::{M1ndError, M1ndResult};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

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
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Handle the `memorize` MCP tool call.
pub fn handle_light_author(state: &mut SessionState, input: LightAuthorInput) -> M1ndResult<Value> {
    // 1. Resolve output path.
    let out_path = resolve_output_path(state, &input)?;

    // 2. Render markdown.
    let markdown = render_light_markdown(&input);

    // 3. Write to disk.
    let parent = out_path.parent().ok_or_else(|| M1ndError::InvalidParams {
        tool: "memorize".into(),
        detail: "output path has no parent directory".into(),
    })?;
    fs::create_dir_all(parent).map_err(M1ndError::Io)?;
    fs::write(&out_path, &markdown).map_err(M1ndError::Io)?;

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
        let resolved = ingest_result["light_evidence_resolved"].as_u64().unwrap_or(0);
        let unresolved = ingest_result["light_evidence_unresolved"].as_u64().unwrap_or(0);

        return Ok(json!({
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
            "rendered": markdown,
        }));
    }

    Ok(json!({
        "ok": true,
        "schema": "m1nd-memorize-v0",
        "path": path_str,
        "bytes_written": bytes_written,
        "claims_written": claims_written,
        "ingested": false,
        "rendered": markdown,
    }))
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
    let state_val = input
        .state
        .as_deref()
        .unwrap_or("authored");
    let title_val = input
        .title
        .as_deref()
        .unwrap_or(input.node_label.as_str());

    let mut out = String::new();

    // Frontmatter
    out.push_str("---\n");
    out.push_str("Protocol: L1GHT/1.0\n");
    out.push_str(&format!("Node: {}\n", input.node_label));
    out.push_str(&format!("State: {}\n", state_val));
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
        SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session")
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
        assert!(md.contains("Protocol: L1GHT/1.0"), "missing protocol header");
        assert!(md.contains("Node: AuthSystem"), "missing Node header");

        // Entity marker is before 𝔻 confidence
        let entity_pos = md.find("[⍂ entity: TokenValidator]").expect("entity marker missing");
        let conf_pos = md.find("[𝔻 confidence: 0.9]").expect("confidence marker missing");
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
    // Test 3: slugify helper
    // -----------------------------------------------------------------------
    #[test]
    fn slugify_lowercases_and_replaces_non_alnum() {
        assert_eq!(slugify("AuthSystem"), "authsystem");
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("foo::bar::baz"), "foo-bar-baz");
        assert_eq!(slugify("  leading"), "-leading");
    }
}
