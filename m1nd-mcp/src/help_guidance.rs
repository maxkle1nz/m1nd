use crate::personality::{
    self, GLYPH_CONNECTION, GLYPH_DIMENSION, GLYPH_PATH, GLYPH_SIGNAL, GLYPH_STRUCTURE,
};
use crate::protocol::layers::{
    HelpGuidance, HelpInput, HelpMinimalCall, HelpMode, HelpRejectedAlternative, HelpRender,
    HelpSequenceStep, HelpStage,
};
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Debug)]
pub struct HelpParamSpec {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub default: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct HelpCatalogEntry {
    pub name: String,
    pub category: String,
    pub glyph: String,
    pub one_liner: String,
    pub returns: String,
    pub params: Vec<HelpParamSpec>,
    pub minimal_arguments: Value,
    pub next: Vec<String>,
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct HelpResolution {
    pub formatted: String,
    pub guidance: Option<HelpGuidance>,
    pub found: bool,
    pub suggestions: Vec<String>,
    pub tool: Option<String>,
    pub proof_state: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeGuidanceProjection {
    pub proof_state: String,
    pub next_suggested_tool: Option<String>,
    pub next_suggested_target: Option<String>,
    pub next_step_hint: Option<String>,
    pub confidence: Option<f32>,
    pub why_this_next_step: Option<String>,
    pub what_is_missing: Option<String>,
}

#[derive(Clone, Debug)]
struct SchemaTool {
    name: String,
    description: String,
    params: Vec<HelpParamSpec>,
    input_schema: Value,
}

#[derive(Clone, Debug)]
struct OverviewCard {
    title: &'static str,
    why: &'static str,
    tool: &'static str,
    arguments: Value,
    reject_tool: &'static str,
    reject_reason: &'static str,
}

pub fn catalog_entries() -> Vec<HelpCatalogEntry> {
    // The registry, the manual and the floor table are all fixed for the life
    // of the process, so the catalog is built once. Every `help` call and every
    // runtime error hint used to rebuild all of it — several times per call.
    static CATALOG: std::sync::OnceLock<Vec<HelpCatalogEntry>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(build_catalog_entries).clone()
}

fn build_catalog_entries() -> Vec<HelpCatalogEntry> {
    let manual_docs: HashMap<String, personality::ToolDoc> = personality::tool_docs()
        .into_iter()
        .map(|doc| (doc.name.to_string(), doc))
        .collect();

    schema_tools()
        .into_iter()
        .map(|schema| {
            let manual = manual_docs.get(&schema.name);
            let (category, glyph) = manual
                .map(|doc| (doc.category.to_string(), doc.glyph.to_string()))
                .unwrap_or_else(|| infer_category_and_glyph(&schema.name));
            // The manual's curated one-liner SHADOWS the schema description, so
            // the floor annotation has to travel with it — otherwise `help`
            // keeps advertising as callable the verbs `tools/list` now marks
            // POLICY-DISABLED. The schema description already carries the long
            // form, so it passes through untouched.
            let one_liner = manual
                .map(|doc| crate::server::annotate_floor_gated_summary(&schema.name, doc.one_liner))
                .unwrap_or_else(|| schema.description.clone());
            let returns = manual
                .map(|doc| doc.returns.to_string())
                .unwrap_or_else(|| {
                    "Structured output documented by the live runtime schema.".into()
                });
            // A next list is a promise that the reader can take the step. It is
            // filtered through the authority floor table, so a verb whose floor
            // rises above what a generic caller holds drops out by itself.
            let next = manual
                .map(|doc| doc.next.iter().map(|next| (*next).to_string()).collect())
                .unwrap_or_else(|| inferred_next_tools(&schema.name))
                .into_iter()
                .filter(|next: &String| crate::server::generic_caller_reaches(next))
                .collect();
            let aliases = vec![
                format!("m1nd.{}", schema.name),
                format!("m1nd_{}", schema.name),
            ];
            let minimal_arguments =
                choose_minimal_arguments(&schema.name, manual, &schema.input_schema);

            HelpCatalogEntry {
                name: schema.name,
                category,
                glyph,
                one_liner,
                returns,
                params: schema.params,
                minimal_arguments,
                next,
                aliases,
            }
        })
        .collect()
}

pub fn catalog_entry(tool_name: &str) -> Option<HelpCatalogEntry> {
    catalog_entries()
        .into_iter()
        .find(|entry| entry.name == tool_name)
}

pub fn live_tool_names() -> Vec<String> {
    catalog_entries()
        .into_iter()
        .map(|entry| entry.name)
        .collect()
}

pub fn find_similar_tool_names(query: &str, max_suggestions: usize) -> Vec<String> {
    let query = query.trim().to_lowercase();
    let mut scored: Vec<(String, usize)> = live_tool_names()
        .into_iter()
        .map(|name| {
            let distance = levenshtein_distance(&query, &name.to_lowercase());
            (name, distance)
        })
        .filter(|(_, distance)| *distance <= 6)
        .collect();
    scored.sort_by_key(|(_, distance)| *distance);
    scored
        .into_iter()
        .take(max_suggestions.max(1))
        .map(|(name, _)| name)
        .collect()
}

pub fn build_tool_resolution(entry: &HelpCatalogEntry, input: &HelpInput) -> HelpResolution {
    let render = input.render.unwrap_or(HelpRender::Full);
    let guidance = tool_guidance(entry, input);
    let formatted = match render {
        HelpRender::None => String::new(),
        HelpRender::Compact => render_tool(entry, &guidance, true),
        HelpRender::Full => render_tool(entry, &guidance, false),
    };

    HelpResolution {
        formatted,
        guidance: Some(guidance),
        found: true,
        suggestions: vec![],
        tool: Some(entry.name.clone()),
        proof_state: "triaging".into(),
    }
}

pub fn build_unknown_tool_resolution(query: &str, max_suggestions: usize) -> HelpResolution {
    let suggestions = find_similar_tool_names(query, max_suggestions);
    let guidance = HelpGuidance {
        decision_type: "recovery".into(),
        recommended_tools: suggestions.clone(),
        recommended_sequence: vec![HelpSequenceStep {
            tool: "help".into(),
            reason: "Retry with a canonical live tool name from the current runtime surface."
                .into(),
        }],
        required_inputs: vec!["tool_name".into()],
        missing_context: vec!["A valid canonical tool name".into()],
        minimal_calls: suggestions
            .first()
            .map(|tool| {
                vec![HelpMinimalCall {
                    tool: "help".into(),
                    arguments: json!({
                        "agent_id": "jimi",
                        "tool_name": tool,
                    }),
                }]
            })
            .unwrap_or_default(),
        rejected_alternatives: vec![HelpRejectedAlternative {
            tool: query.to_string(),
            reason: "That name does not resolve to a live canonical tool in this runtime.".into(),
        }],
        recovery_steps: vec![
            "Use one of the suggested canonical names.".into(),
            "If you do not know the tool name, call help with stage or intent instead of guessing."
                .into(),
        ],
        next_step: suggestions
            .first()
            .map(|tool| format!("Retry help with tool_name=\"{}\".", tool)),
        why: "The requested tool name did not match the live m1nd tool surface.".into(),
        confidence: 0.36,
        canonical_name: None,
        aliases: vec![],
    };

    HelpResolution {
        formatted: render_unknown_tool(query, &suggestions),
        guidance: Some(guidance),
        found: false,
        suggestions,
        tool: Some(query.to_string()),
        proof_state: "blocked".into(),
    }
}

pub fn build_overview_resolution(input: &HelpInput, show_temponizer: bool) -> HelpResolution {
    let total_tools = live_tool_names().len();
    let cards = overview_cards(input);
    let formatted = match input.render.unwrap_or(HelpRender::Full) {
        HelpRender::None => String::new(),
        HelpRender::Compact => render_overview(total_tools, &cards, true, show_temponizer),
        HelpRender::Full => render_overview(total_tools, &cards, false, show_temponizer),
    };
    let guidance = HelpGuidance {
        decision_type: "overview".into(),
        recommended_tools: cards.iter().map(|card| card.tool.to_string()).collect(),
        recommended_sequence: cards
            .iter()
            .take(3)
            .map(|card| HelpSequenceStep {
                tool: card.tool.to_string(),
                reason: card.why.into(),
            })
            .collect(),
        required_inputs: vec![],
        missing_context: vec!["Pick a stage, a tool, or an intent to get a tighter route.".into()],
        minimal_calls: cards
            .iter()
            .take(3)
            .map(|card| HelpMinimalCall {
                tool: card.tool.to_string(),
                arguments: card.arguments.clone(),
            })
            .collect(),
        rejected_alternatives: vec![HelpRejectedAlternative {
            tool: "blind file hunting".into(),
            reason: "The help overview is designed to route by agent state before shell search."
                .into(),
        }],
        recovery_steps: vec![
            "Describe the stage or paste the error instead of guessing a tool.".into(),
        ],
        next_step: Some(
            "Choose the card that matches your current state, then open tool help if needed."
                .into(),
        ),
        why: "Overview mode is a router for agent state, not a memorization-heavy catalog.".into(),
        confidence: 0.61,
        canonical_name: None,
        aliases: vec![],
    };

    HelpResolution {
        formatted,
        guidance: Some(guidance),
        found: true,
        suggestions: vec![],
        tool: None,
        proof_state: "triaging".into(),
    }
}

pub fn build_route_resolution(input: &HelpInput) -> HelpResolution {
    let guidance = route_guidance(input, false);
    let formatted = match input.render.unwrap_or(HelpRender::Full) {
        HelpRender::None => String::new(),
        HelpRender::Compact => render_guidance("ROUTE", &guidance, true),
        HelpRender::Full => render_guidance("ROUTE", &guidance, false),
    };

    HelpResolution {
        formatted,
        guidance: Some(guidance),
        found: true,
        suggestions: vec![],
        tool: None,
        proof_state: "triaging".into(),
    }
}

pub fn build_workflow_resolution(input: &HelpInput) -> HelpResolution {
    let guidance = route_guidance(input, true);
    let formatted = match input.render.unwrap_or(HelpRender::Full) {
        HelpRender::None => String::new(),
        HelpRender::Compact => render_guidance("WORKFLOW", &guidance, true),
        HelpRender::Full => render_guidance("WORKFLOW", &guidance, false),
    };

    HelpResolution {
        formatted,
        guidance: Some(guidance),
        found: true,
        suggestions: vec![],
        tool: None,
        proof_state: "triaging".into(),
    }
}

pub fn build_recovery_resolution(input: &HelpInput) -> HelpResolution {
    let guidance = recovery_guidance(input);
    let formatted = match input.render.unwrap_or(HelpRender::Full) {
        HelpRender::None => String::new(),
        HelpRender::Compact => render_guidance("RECOVERY", &guidance, true),
        HelpRender::Full => render_guidance("RECOVERY", &guidance, false),
    };

    HelpResolution {
        formatted,
        guidance: Some(guidance),
        found: true,
        suggestions: vec![],
        tool: input.tool_name.clone(),
        proof_state: "blocked".into(),
    }
}

pub fn runtime_projection_from_guidance(
    guidance: Option<&HelpGuidance>,
    proof_state: impl Into<String>,
) -> RuntimeGuidanceProjection {
    let proof_state = proof_state.into();
    let next_suggested_tool = guidance.and_then(|guidance| {
        guidance
            .recommended_sequence
            .first()
            .map(|step| step.tool.clone())
            .or_else(|| guidance.recommended_tools.first().cloned())
    });
    let next_suggested_target = guidance.and_then(|guidance| {
        if !guidance.missing_context.is_empty() {
            Some(guidance.missing_context.join(", "))
        } else {
            guidance
                .canonical_name
                .clone()
                .or_else(|| guidance.aliases.first().cloned())
        }
    });
    let next_step_hint = guidance.and_then(|guidance| guidance.next_step.clone());
    let confidence = guidance.map(|guidance| guidance.confidence);
    let why_this_next_step = guidance.map(|guidance| guidance.why.clone());
    let what_is_missing = guidance.and_then(|guidance| {
        if guidance.missing_context.is_empty() {
            None
        } else {
            Some(guidance.missing_context.join(", "))
        }
    });

    RuntimeGuidanceProjection {
        proof_state,
        next_suggested_tool,
        next_suggested_target,
        next_step_hint,
        confidence,
        why_this_next_step,
        what_is_missing,
    }
}

pub fn runtime_projection_from_resolution(
    resolution: &HelpResolution,
    default_proof_state: &str,
) -> RuntimeGuidanceProjection {
    let proof_state = if resolution.proof_state.is_empty() {
        default_proof_state.to_string()
    } else {
        resolution.proof_state.clone()
    };
    runtime_projection_from_guidance(resolution.guidance.as_ref(), proof_state)
}

pub fn runtime_projection_for_tool(
    tool_name: &str,
    input: &HelpInput,
    default_proof_state: &str,
) -> Option<RuntimeGuidanceProjection> {
    let canonical = normalize_tool_name(tool_name);
    let entry = catalog_entry(&canonical)?;
    let mut normalized_input = input.clone();
    normalized_input.tool_name = Some(canonical);
    normalized_input.render = Some(HelpRender::None);
    let resolution = build_tool_resolution(&entry, &normalized_input);
    Some(runtime_projection_from_resolution(
        &resolution,
        default_proof_state,
    ))
}

pub fn runtime_error_guidance_hint(tool_name: &str, detail: &str) -> String {
    let canonical = normalize_tool_name(tool_name);
    let input = HelpInput {
        agent_id: "runtime".into(),
        tool_name: Some(canonical.clone()),
        mode: Some(HelpMode::Recovery),
        intent: None,
        stage: None,
        path: None,
        error_text: Some(detail.to_string()),
        recent_tools: vec![],
        max_suggestions: Some(3),
        render: Some(HelpRender::None),
    };
    let resolution = build_recovery_resolution(&input);
    let projection = runtime_projection_from_resolution(&resolution, "blocked");

    let mut parts = vec![detail.trim().to_string()];
    if let Some(call) = resolution
        .guidance
        .as_ref()
        .and_then(|guidance| guidance.minimal_calls.first())
    {
        parts.push(format!(
            "minimal call: {}({})",
            call.tool,
            render_inline_json(&call.arguments)
        ));
    }
    if let Some(next_step) = projection.next_step_hint {
        parts.push(format!("next: {next_step}"));
    }
    if let Some(missing) = projection.what_is_missing {
        parts.push(format!("missing: {missing}"));
    }

    parts.join("; ")
}

/// The catalog `help` reasons over: THE REGISTRY, never the advertised menu.
///
/// `help` is the door to every verb `tools/list` no longer shows, so reading the
/// tier-gated list here would have opened that door onto the same small room —
/// and the fuzzy "did you mean?" matcher behind every mistyped call could not
/// have named a hidden verb even when the agent typed it almost right. The gap
/// was already on the record as low severity in `docs/uml/tool-surface.md`;
/// shrinking the default menu to a core promoted it to load-bearing. Pinned by
/// `server::tests::help_sees_every_verb_regardless_of_tier`.
fn schema_tools() -> Vec<SchemaTool> {
    crate::server::all_tool_schemas()
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|tool| {
            let name = tool.get("name")?.as_str()?.to_string();
            let description = tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let input_schema = tool
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let params = schema_params(&input_schema);
            Some(SchemaTool {
                name,
                description,
                params,
                input_schema,
            })
        })
        .collect()
}

fn normalize_tool_name(tool_name: &str) -> String {
    let trimmed = tool_name.trim();
    trimmed
        .strip_prefix("m1nd.")
        .or_else(|| trimmed.strip_prefix("m1nd_"))
        .unwrap_or(trimmed)
        .to_string()
}

fn schema_params(input_schema: &Value) -> Vec<HelpParamSpec> {
    let required = input_schema
        .get("required")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();

    input_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .iter()
                .map(|(name, schema)| HelpParamSpec {
                    name: name.to_string(),
                    description: schema
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    required: required.contains(name),
                    default: schema.get("default").cloned(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn choose_minimal_arguments(
    tool_name: &str,
    manual: Option<&personality::ToolDoc>,
    input_schema: &Value,
) -> Value {
    if let Some(override_value) = minimal_call_override(tool_name) {
        return override_value;
    }

    if let Some(manual) = manual {
        if let Ok(parsed) = serde_json::from_str::<Value>(manual.example) {
            if validate_against_schema(&parsed, input_schema) {
                return parsed;
            }
        }
    }

    build_minimal_arguments_from_schema(tool_name, input_schema)
}

fn build_minimal_arguments_from_schema(tool_name: &str, input_schema: &Value) -> Value {
    let required = input_schema
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let properties = input_schema
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut object = Map::new();
    for key in required.iter().filter_map(Value::as_str) {
        if let Some(schema) = properties.get(key) {
            object.insert(key.to_string(), example_value_for(tool_name, key, schema));
        }
    }
    Value::Object(object)
}

fn example_value_for(tool_name: &str, param_name: &str, schema: &Value) -> Value {
    if let Some(default) = schema.get("default") {
        return default.clone();
    }
    if let Some(variants) = schema.get("enum").and_then(Value::as_array) {
        if let Some(value) = variants.first() {
            return value.clone();
        }
    }

    match schema.get("type").and_then(Value::as_str) {
        Some("string") => Value::String(guess_string(tool_name, param_name)),
        Some("integer") => Value::Number(guess_integer(param_name).into()),
        Some("number") => json!(guess_number(param_name)),
        Some("boolean") => Value::Bool(guess_boolean(param_name)),
        Some("array") => {
            let item = schema
                .get("items")
                .map(|items| example_value_for(tool_name, param_name, items))
                .unwrap_or_else(|| Value::String("example".into()));
            Value::Array(vec![item])
        }
        Some("object") => {
            let required = schema
                .get("required")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let properties = schema
                .get("properties")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mut object = Map::new();
            for key in required.iter().filter_map(Value::as_str) {
                if let Some(child) = properties.get(key) {
                    object.insert(key.to_string(), example_value_for(tool_name, key, child));
                }
            }
            Value::Object(object)
        }
        _ => Value::String("example".into()),
    }
}

fn guess_string(tool_name: &str, param_name: &str) -> String {
    match param_name {
        "agent_id" => "jimi".into(),
        "query" => match tool_name {
            "search" => "authentication middleware".into(),
            "seek" => "where retry backoff is decided".into(),
            "activate" => "payment pipeline".into(),
            "missing" => "session invalidation".into(),
            _ => "authentication flow".into(),
        },
        "pattern" => "**/*.rs".into(),
        "path" => match tool_name {
            "audit" => "/path/to/project".into(),
            "document_resolve" | "document_bindings" | "document_drift" => "docs/spec.md".into(),
            _ => "/path/to/input".into(),
        },
        "scope" => "src".into(),
        "file_path" => "src/example.rs".into(),
        "new_content" | "content" => "...".into(),
        "description" => "Apply the validated change".into(),
        "node_id" | "source" | "target" | "changed_node" | "anchor_node" => {
            "file::src/main.rs".into()
        }
        "trail_id" => "trail_auth_flow".into(),
        "preview_id" => "preview_jimi_1".into(),
        "perspective_id" => "persp_auth".into(),
        "route_id" => "route_auth_1".into(),
        "error_text" => "TypeError: undefined is not a function".into(),
        "tool_name" => "activate".into(),
        "branch_name" => "follow-up".into(),
        "service_name" => "auth-service".into(),
        "current_repo_name" => "current-repo".into(),
        "namespace" => "docs".into(),
        _ if param_name.contains("path") => "/path/to/input".into(),
        _ if param_name.contains("file") => "src/example.rs".into(),
        _ => "example".into(),
    }
}

fn guess_integer(param_name: &str) -> i64 {
    match param_name {
        "route_set_version" | "route_index" | "page" => 1,
        "page_size" => 6,
        "top_k" | "max_results" | "max_repos" | "max_output_chars" => 5,
        "debounce_ms" => 200,
        _ => 1,
    }
}

fn guess_number(param_name: &str) -> f64 {
    match param_name {
        "similarity_threshold" => 0.85,
        "min_score" => 0.3,
        "boost_strength" => 0.15,
        _ => 1.0,
    }
}

fn guess_boolean(param_name: &str) -> bool {
    match param_name {
        "atomic" | "reingest" | "verify" => true,
        _ => true,
    }
}

fn minimal_call_override(tool_name: &str) -> Option<Value> {
    match tool_name {
        "help" => Some(json!({
            "agent_id": "jimi",
            "stage": "plan",
            "intent": "validate a risky change before editing"
        })),
        "apply_batch" => Some(json!({
            "agent_id": "jimi",
            "edits": [{
                "file_path": "src/example.rs",
                "new_content": "...",
                "description": "Apply the validated change"
            }],
            "atomic": true,
            "reingest": true,
            "verify": true
        })),
        "document_resolve" | "document_bindings" | "document_drift" => Some(json!({
            "agent_id": "jimi",
            "path": "docs/spec.md"
        })),
        "perspective_inspect"
        | "perspective_peek"
        | "perspective_follow"
        | "perspective_affinity" => Some(json!({
            "agent_id": "jimi",
            "perspective_id": "persp_auth",
            "route_set_version": 1,
            "route_index": 1
        })),
        "runtime_overlay" => Some(json!({
            "agent_id": "jimi",
            "spans": [{
                "name": "auth.request",
                "duration_us": 1200
            }]
        })),
        "auto_ingest_start" => Some(json!({
            "agent_id": "jimi",
            "roots": ["docs"]
        })),
        // The schema's first enum value is the retired `set`; demonstrate the
        // read a generic caller can actually make.
        "boot_memory" => Some(json!({
            "agent_id": "jimi",
            "action": "list"
        })),
        "daemon_start" => Some(json!({
            "agent_id": "jimi",
            "watch_paths": ["/path/to/project"]
        })),
        _ => None,
    }
}

fn validate_against_schema(value: &Value, schema: &Value) -> bool {
    if let Some(variants) = schema.get("enum").and_then(Value::as_array) {
        if !variants.iter().any(|candidate| candidate == value) {
            return false;
        }
    }

    match schema.get("type").and_then(Value::as_str) {
        Some("object") => {
            let Some(object) = value.as_object() else {
                return false;
            };
            let properties = schema
                .get("properties")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let required = schema
                .get("required")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return false;
                }
            }
            if properties.is_empty() {
                return true;
            }
            for (key, child_value) in object {
                if let Some(child_schema) = properties.get(key) {
                    if !validate_against_schema(child_value, child_schema) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            true
        }
        Some("array") => {
            let Some(values) = value.as_array() else {
                return false;
            };
            let Some(item_schema) = schema.get("items") else {
                return true;
            };
            values
                .iter()
                .all(|entry| validate_against_schema(entry, item_schema))
        }
        Some("string") => value.is_string(),
        Some("integer") => value.as_i64().is_some() || value.as_u64().is_some(),
        Some("number") => value.is_number(),
        Some("boolean") => value.is_boolean(),
        _ => true,
    }
}

fn infer_category_and_glyph(tool_name: &str) -> (String, String) {
    let (category, glyph) = match tool_name {
        name if name.starts_with("document_") || name.starts_with("auto_ingest_") => {
            ("Docs", GLYPH_DIMENSION)
        }
        name if name.starts_with("daemon_")
            || name.starts_with("alerts_")
            || matches!(name, "metrics" | "persist" | "diagram") =>
        {
            ("Operations", GLYPH_SIGNAL)
        }
        name if name.starts_with("trail_") || name == "boot_memory" => ("Trail", GLYPH_PATH),
        name if name.starts_with("lock_") => ("Lock", GLYPH_CONNECTION),
        name if name.starts_with("perspective_") => ("Perspective", GLYPH_PATH),
        "heuristics_surface" | "type_trace" => ("Superpowers", GLYPH_STRUCTURE),
        _ => ("Extended", GLYPH_SIGNAL),
    };
    (category.into(), glyph.into())
}

fn inferred_next_tools(tool_name: &str) -> Vec<String> {
    match tool_name {
        "document_provider_health" => vec!["document_resolve".into(), "document_bindings".into()],
        "document_resolve" => vec!["document_bindings".into(), "document_drift".into()],
        "document_bindings" => vec!["document_drift".into(), "search".into()],
        "document_drift" => vec!["document_bindings".into(), "cross_verify".into()],
        "auto_ingest_start" => vec!["auto_ingest_status".into(), "document_bindings".into()],
        "auto_ingest_tick" | "auto_ingest_status" => {
            vec!["document_bindings".into(), "document_drift".into()]
        }
        "daemon_start" => vec!["daemon_status".into(), "alerts_list".into()],
        "daemon_status" => vec!["alerts_list".into(), "health".into()],
        "alerts_list" => vec!["alerts_ack".into(), "daemon_status".into()],
        "boot_memory" => vec!["north".into(), "trail_save".into()],
        "heuristics_surface" => vec!["batch_view".into(), "validate_plan".into()],
        "type_trace" => vec!["taint_trace".into(), "validate_plan".into()],
        "metrics" | "persist" => vec!["report".into(), "health".into()],
        "diagram" => vec!["panoramic".into(), "layers".into()],
        _ => vec!["help".into()],
    }
}

fn tool_guidance(entry: &HelpCatalogEntry, input: &HelpInput) -> HelpGuidance {
    let required_inputs: Vec<String> = entry
        .params
        .iter()
        .filter(|param| param.required)
        .map(|param| param.name.clone())
        .collect();
    let missing_context = missing_context_for_tool(entry, input);
    let rejected_alternatives = likely_wrong_tool(entry)
        .map(|(tool, reason)| {
            vec![HelpRejectedAlternative {
                tool: tool.into(),
                reason: reason.into(),
            }]
        })
        .unwrap_or_default();
    let recovery_steps = tool_recovery_steps(entry, input);
    let why = format!(
        "{} {}",
        entry.one_liner.trim_end_matches('.'),
        if entry.returns.is_empty() {
            String::new()
        } else {
            format!("Returns: {}.", entry.returns.trim_end_matches('.'))
        }
    )
    .trim()
    .to_string();

    HelpGuidance {
        decision_type: "tool".into(),
        recommended_tools: vec![entry.name.clone()],
        recommended_sequence: entry
            .next
            .iter()
            .take(3)
            .enumerate()
            .map(|(index, tool)| HelpSequenceStep {
                tool: tool.clone(),
                reason: if index == 0 {
                    format!("Typical downstream follow-up after `{}`.", entry.name)
                } else {
                    format!("Another follow-up after `{}`.", entry.name)
                },
            })
            .collect(),
        required_inputs,
        missing_context,
        minimal_calls: vec![HelpMinimalCall {
            tool: entry.name.clone(),
            arguments: entry.minimal_arguments.clone(),
        }],
        rejected_alternatives,
        recovery_steps,
        next_step: entry
            .next
            .first()
            .map(|tool| format!("Run `{}` if this tool finishes cleanly.", tool)),
        why,
        confidence: 0.88,
        canonical_name: Some(entry.name.clone()),
        aliases: entry.aliases.clone(),
    }
}

fn missing_context_for_tool(entry: &HelpCatalogEntry, input: &HelpInput) -> Vec<String> {
    let mut missing = Vec::new();
    if entry
        .params
        .iter()
        .any(|param| param.required && param.name == "path")
        && input.path.is_none()
        && !entry
            .minimal_arguments
            .get("path")
            .map(Value::is_string)
            .unwrap_or(false)
    {
        missing.push("path".into());
    }
    if entry.name == "trace" && input.error_text.is_none() {
        missing.push("error_text".into());
    }
    missing
}

fn likely_wrong_tool(entry: &HelpCatalogEntry) -> Option<(&'static str, &'static str)> {
    match entry.name.as_str() {
        "search" => Some((
            "seek",
            "Use `search` only when the text or regex is already known.",
        )),
        "seek" => Some((
            "search",
            "Use `seek` when the purpose is known but the exact location is not.",
        )),
        "glob" => Some((
            "search",
            "Use `glob` when the problem is mainly about filenames or path patterns.",
        )),
        "trace" => Some((
            "search",
            "Use `trace` when you already have failure text and need the best next file, not a raw text search.",
        )),
        "validate_plan" => Some((
            "blind edit",
            "Do not write a connected edit before the plan has passed structural validation.",
        )),
        "apply_batch" => Some((
            "surgical_context_v2",
            "Do not use `apply_batch` while you are still discovering the edit surface.",
        )),
        _ => None,
    }
}

fn tool_recovery_steps(entry: &HelpCatalogEntry, input: &HelpInput) -> Vec<String> {
    let mut steps: Vec<String> = personality::error_recovery_notes(&entry.name)
        .iter()
        .map(|line| (*line).to_string())
        .collect();
    if entry.name == "trace" && input.error_text.is_none() {
        steps
            .push("Paste the error text or stacktrace so trace can route you structurally.".into());
    }
    if steps.is_empty() {
        steps.push("Read the missing inputs and retry with the canonical tool contract.".into());
    }
    steps
}

fn route_guidance(input: &HelpInput, workflow: bool) -> HelpGuidance {
    let intent = input.intent.as_deref().unwrap_or_default().to_lowercase();
    if (intent.contains("graph") && (intent.contains("change") || intent.contains("watch")))
        || intent.contains("track changes")
    {
        if let Some(entry) = catalog_entry("lock_create") {
            let mut guidance = tool_guidance(&entry, input);
            guidance.decision_type = if workflow { "workflow" } else { "route" }.into();
            guidance.recommended_tools =
                vec!["seek".into(), "lock_create".into(), "lock_diff".into()];
            guidance.recommended_sequence = [
                ("seek", "Resolve a live canonical node ID before creating a baseline."),
                ("lock_create", "Capture a session-local node/subgraph/path baseline using that ID."),
                ("lock_watch", "Optionally buffer on_ingest or on_learn events; this does not authorize those mutations."),
                ("lock_diff", "After an independently authorized graph update, inspect internal and boundary changes."),
                ("lock_rebase", "Only after reviewing the diff, accept the current graph as the new baseline."),
                ("lock_release", "Discard the baseline when the investigation ends."),
            ].into_iter().map(|(tool, reason)| HelpSequenceStep { tool: tool.into(), reason: reason.into() }).collect();
            guidance.minimal_calls = vec![HelpMinimalCall {
                tool: "seek".into(),
                arguments: json!({"agent_id": input.agent_id, "query": input.intent}),
            }];
            guidance.missing_context = vec!["A live canonical root node ID from seek/search; subsequent calls must reuse the returned lock_id and agent_id.".into()];
            guidance.why = "Graph change tracking uses ephemeral snapshots, not filesystem locks, source-text diffs, or mutual exclusion.".into();
            guidance.recovery_steps = vec!["Use M1ND_TOOL_TIER=investigate on the server and reconnect if your host can call only listed tools. Help alone does not register a host tool.".into()];
            guidance.next_step = Some("Resolve the target with seek, then use help(tool_name=lock_create) for the exact schema.".into());
            return guidance;
        }
    }
    let stage = input
        .stage
        .or_else(|| infer_stage_from_intent(input.intent.as_deref()))
        .unwrap_or(HelpStage::Find);
    let recent = normalize_recent_tools(&input.recent_tools);
    let intent = input.intent.as_deref().unwrap_or_default();

    match stage {
        HelpStage::Orient => {
            let task = if intent.is_empty() {
                "understand this repository before editing".to_string()
            } else {
                intent.to_string()
            };
            HelpGuidance {
                decision_type: if workflow { "sequence" } else { "tool" }.into(),
                recommended_tools: vec!["north".into()],
                recommended_sequence: vec![
                    HelpSequenceStep {
                        tool: "north".into(),
                        reason: "One call returns binding trust, the focus nodes for your task, durable memory, and the source of the top nodes.".into(),
                    },
                    HelpSequenceStep {
                        tool: "batch_view".into(),
                        reason: "Open the remaining focus files north named in one read.".into(),
                    },
                ],
                required_inputs: vec!["task".into()],
                missing_context: if intent.is_empty() {
                    vec!["task".into()]
                } else {
                    Vec::new()
                },
                minimal_calls: vec![HelpMinimalCall {
                    tool: "north".into(),
                    arguments: json!({
                        "agent_id": "jimi",
                        "task": task
                    }),
                }],
                rejected_alternatives: vec![HelpRejectedAlternative {
                    tool: "search".into(),
                    reason: "Do not start with text search when you do not yet know the repo shape or likely surface.".into(),
                }],
                recovery_steps: vec![
                    "If north reports needs_ingest or a caller_root_mismatch, read its repair before trusting any retrieval.".into(),
                    "If the focus is still too broad, use batch_view or activate on the strongest focus node.".into(),
                ],
                next_step: Some("Run north with your task first, then inspect the strongest focus node.".into()),
                why: "Orientation work should start from one honest packet about the graph and your task, not a blind string hunt.".into(),
                confidence: 0.92,
                canonical_name: None,
                aliases: vec![],
            }
        }
        HelpStage::Ground => HelpGuidance {
            decision_type: "sequence".into(),
            recommended_tools: vec![
                "document_resolve".into(),
                "document_bindings".into(),
                "document_drift".into(),
            ],
            recommended_sequence: vec![
                HelpSequenceStep {
                    tool: "document_resolve".into(),
                    reason: "Resolve the canonical artifact for a document the graph already holds.".into(),
                },
                HelpSequenceStep {
                    tool: "document_bindings".into(),
                    reason: "Bind the document's claims back to the implementing code.".into(),
                },
                HelpSequenceStep {
                    tool: "document_drift".into(),
                    reason: "Check where the code has drifted from what the document says.".into(),
                },
            ],
            required_inputs: vec!["path".into()],
            missing_context: input
                .path
                .as_ref()
                .map(|_| Vec::new())
                .unwrap_or_else(|| vec!["path".into()]),
            minimal_calls: vec![HelpMinimalCall {
                tool: "document_resolve".into(),
                arguments: json!({
                    "agent_id": "jimi",
                    "path": input.path.clone().unwrap_or_else(|| "docs/spec.md".into())
                }),
            }],
            rejected_alternatives: vec![HelpRejectedAlternative {
                tool: "search".into(),
                reason: "Do not search a spec blind when the graph can bind it to code and report drift.".into(),
            }],
            recovery_steps: vec![
                "If document_resolve cannot find the document, the graph does not hold it yet. Grounding a NEW document is a graph write above the generic authority floor; ingest {mode: \"refresh\"} from the brain's exact declared root re-scans only what that root already declares.".into(),
                "Until the document is in the graph, read it directly and use seek or search to find the code it describes.".into(),
            ],
            next_step: Some("Resolve the document, then bind it back to code.".into()),
            why: "Docs/spec work is strongest when the document is already grounded in the graph; resolving and binding it are reads any caller can make.".into(),
            confidence: 0.86,
            canonical_name: None,
            aliases: vec![],
        },
        HelpStage::Diagnose => HelpGuidance {
            decision_type: "sequence".into(),
            recommended_tools: vec!["trace".into(), "view".into(), "impact".into()],
            recommended_sequence: vec![
                HelpSequenceStep {
                    tool: "trace".into(),
                    reason: "Turn failure text into the strongest next inspection target.".into(),
                },
                HelpSequenceStep {
                    tool: "view".into(),
                    reason: "Open the suggested file or seam that trace identifies.".into(),
                },
                HelpSequenceStep {
                    tool: "impact".into(),
                    reason: "Check the downstream blast radius if the seam looks risky.".into(),
                },
            ],
            required_inputs: vec!["error_text".into()],
            missing_context: input
                .error_text
                .as_ref()
                .map(|_| Vec::new())
                .unwrap_or_else(|| vec!["error_text".into()]),
            minimal_calls: vec![HelpMinimalCall {
                tool: "trace".into(),
                arguments: json!({
                    "agent_id": "jimi",
                    "error_text": input
                        .error_text
                        .clone()
                        .unwrap_or_else(|| "TypeError: undefined is not a function".into())
                }),
            }],
            rejected_alternatives: vec![HelpRejectedAlternative {
                tool: "search".into(),
                reason: "Failure text already contains structure; let trace turn it into a route first.".into(),
            }],
            recovery_steps: vec!["If trace points at a broad seam, open it with view and then narrow further with search or impact.".into()],
            next_step: Some("Run trace with the failure text, then inspect the suggested seam.".into()),
            why: "Diagnosis is best routed from the observed failure, not from manual keyword guessing.".into(),
            confidence: 0.94,
            canonical_name: None,
            aliases: vec![],
        },
        HelpStage::Plan => {
            let first_tool = if recent.contains("impact") || recent.contains("predict") {
                "validate_plan"
            } else {
                "impact"
            };
            let sequence = if first_tool == "validate_plan" {
                vec![
                    HelpSequenceStep {
                        tool: "validate_plan".into(),
                        reason: "The flow already has blast-radius evidence; validate the connected plan next.".into(),
                    },
                    HelpSequenceStep {
                        tool: "surgical_context_v2".into(),
                        reason: "Ground the compact proof packet if the plan still feels implicit.".into(),
                    },
                ]
            } else {
                vec![
                    HelpSequenceStep {
                        tool: "impact".into(),
                        reason: "Measure blast radius before deciding that the change is locally safe.".into(),
                    },
                    HelpSequenceStep {
                        tool: "predict".into(),
                        reason: "Check historical co-change pressure before freezing the plan.".into(),
                    },
                    HelpSequenceStep {
                        tool: "validate_plan".into(),
                        reason: "Validate the proposed file/action set against the graph.".into(),
                    },
                ]
            };
            HelpGuidance {
                decision_type: "sequence".into(),
                recommended_tools: sequence.iter().map(|step| step.tool.clone()).collect(),
                recommended_sequence: sequence,
                required_inputs: if first_tool == "validate_plan" {
                    vec!["plan".into()]
                } else {
                    vec!["node_id".into()]
                },
                missing_context: if first_tool == "validate_plan" {
                    vec!["plan".into()]
                } else {
                    vec!["node_id or affected seam".into()]
                },
                minimal_calls: vec![if first_tool == "validate_plan" {
                    HelpMinimalCall {
                        tool: "validate_plan".into(),
                        arguments: json!({
                            "agent_id": "jimi",
                            "plan": [{
                                "file": "src/example.rs",
                                "action": "modify"
                            }]
                        }),
                    }
                } else {
                    HelpMinimalCall {
                        tool: "impact".into(),
                        arguments: json!({
                            "agent_id": "jimi",
                            "node_id": "file::src/example.rs"
                        }),
                    }
                }],
                rejected_alternatives: vec![HelpRejectedAlternative {
                    tool: "blind edit".into(),
                    reason: "Do not write before impact and plan validity are strong enough.".into(),
                }],
                recovery_steps: vec!["If the plan is still vague, use surgical_context_v2 to ground the coupled surface before validating again.".into()],
                next_step: Some(format!("Run `{}` first for this planning stage.", first_tool)),
                why: "Planning mode should prove the change surface before any write surface is allowed to execute.".into(),
                confidence: 0.86,
                canonical_name: None,
                aliases: vec![],
            }
        }
        HelpStage::Edit => {
            let ready_to_execute = recent.contains("validate_plan") || recent.contains("surgical_context_v2");
            if ready_to_execute {
                HelpGuidance {
                    decision_type: "sequence".into(),
                    recommended_tools: vec![
                        "edit_preview".into(),
                        "am_i_stale".into(),
                        "predict".into(),
                    ],
                    recommended_sequence: vec![
                        HelpSequenceStep {
                            tool: "edit_preview".into(),
                            reason: "The flow already passed through proof; preview the diff and its validation before you write it.".into(),
                        },
                        HelpSequenceStep {
                            tool: "am_i_stale".into(),
                            reason: "After you write the change with your own editor, see which graph facts went stale.".into(),
                        },
                        HelpSequenceStep {
                            tool: "predict".into(),
                            reason: "Check ripple effects on the files the change touched.".into(),
                        },
                    ],
                    required_inputs: vec!["file_path".into(), "new_content".into()],
                    missing_context: vec!["file_path".into(), "new_content".into()],
                    minimal_calls: vec![HelpMinimalCall {
                        tool: "edit_preview".into(),
                        arguments: json!({
                            "agent_id": "jimi",
                            "file_path": "src/example.rs",
                            "new_content": "..."
                        }),
                    }],
                    rejected_alternatives: vec![HelpRejectedAlternative {
                        tool: "search".into(),
                        reason: "The search phase should already be over if you are at execution.".into(),
                    }],
                    recovery_steps: vec![
                        "m1nd's own source-write verbs sit above the generic authority floor: write the change with your editor, then let am_i_stale tell you what to re-read.".into(),
                        "If execution still feels premature, step back to surgical_context_v2 or validate_plan instead of forcing the write.".into(),
                    ],
                    next_step: Some("Preview the validated change with edit_preview, write it, then run am_i_stale.".into()),
                    why: "Edit mode can execute once the proof surface is already grounded and validated.".into(),
                    confidence: 0.87,
                    canonical_name: None,
                    aliases: vec![],
                }
            } else {
                HelpGuidance {
                    decision_type: "sequence".into(),
                    recommended_tools: vec![
                        "surgical_context_v2".into(),
                        "validate_plan".into(),
                        "edit_preview".into(),
                    ],
                    recommended_sequence: vec![
                        HelpSequenceStep {
                            tool: "surgical_context_v2".into(),
                            reason: "Ground the compact connected edit packet before writing.".into(),
                        },
                        HelpSequenceStep {
                            tool: "validate_plan".into(),
                            reason: "Validate the proposed change set once the surface is explicit.".into(),
                        },
                        HelpSequenceStep {
                            tool: "edit_preview".into(),
                            reason: "Preview the change only after the plan is no longer proving.".into(),
                        },
                    ],
                    required_inputs: vec!["query".into()],
                    missing_context: vec!["query".into()],
                    minimal_calls: vec![HelpMinimalCall {
                        tool: "surgical_context_v2".into(),
                        arguments: json!({
                            "agent_id": "jimi",
                            "query": input
                                .intent
                                .clone()
                                .unwrap_or_else(|| "chat handler message routing".into())
                        }),
                    }],
                    rejected_alternatives: vec![HelpRejectedAlternative {
                        tool: "blind edit".into(),
                        reason: "Do not write while the edit surface is still implicit.".into(),
                    }],
                    recovery_steps: vec!["If you already know the exact file set, move to validate_plan before executing.".into()],
                    next_step: Some("Ground the edit surface with surgical_context_v2.".into()),
                    why: "Edit mode starts with proof-focused grounding unless the flow already passed validation.".into(),
                    confidence: 0.85,
                    canonical_name: None,
                    aliases: vec![],
                }
            }
        }
        HelpStage::Review => HelpGuidance {
            decision_type: "sequence".into(),
            recommended_tools: vec!["timeline".into(), "impact".into(), "batch_view".into()],
            recommended_sequence: vec![
                HelpSequenceStep {
                    tool: "timeline".into(),
                    reason: "Start with recent change history when the task is to review existing change.".into(),
                },
                HelpSequenceStep {
                    tool: "impact".into(),
                    reason: "Measure blast radius on the seam that changed.".into(),
                },
                HelpSequenceStep {
                    tool: "batch_view".into(),
                    reason: "Read the tight set of files once history and impact narrow the surface.".into(),
                },
            ],
            required_inputs: vec!["path".into()],
            missing_context: input
                .path
                .as_ref()
                .map(|_| Vec::new())
                .unwrap_or_else(|| vec!["path".into()]),
            minimal_calls: vec![HelpMinimalCall {
                tool: "timeline".into(),
                arguments: json!({
                    "agent_id": "jimi",
                    "path": input.path.clone().unwrap_or_else(|| "src/example.rs".into())
                }),
            }],
            rejected_alternatives: vec![HelpRejectedAlternative {
                tool: "search".into(),
                reason: "Review is about changed seams and history, not a fresh blind query.".into(),
            }],
            recovery_steps: vec!["If you do not yet know the changed seam, start from north(task) instead.".into()],
            next_step: Some("Run timeline on the changed seam, then inspect impact.".into()),
            why: "Review mode should pull in history and blast radius before opening a broad file set.".into(),
            confidence: 0.8,
            canonical_name: None,
            aliases: vec![],
        },
        HelpStage::Operate => HelpGuidance {
            decision_type: "sequence".into(),
            recommended_tools: vec!["health".into(), "daemon_status".into(), "alerts_list".into()],
            recommended_sequence: vec![
                HelpSequenceStep {
                    tool: "health".into(),
                    reason: "Get the current runtime and graph state in one read.".into(),
                },
                HelpSequenceStep {
                    tool: "daemon_status".into(),
                    reason: "Check whether structural monitoring is already active.".into(),
                },
                HelpSequenceStep {
                    tool: "alerts_list".into(),
                    reason: "Surface pending alerts if the daemon is already watching.".into(),
                },
            ],
            required_inputs: vec![],
            missing_context: vec![],
            minimal_calls: vec![HelpMinimalCall {
                tool: "health".into(),
                arguments: json!({
                    "agent_id": "jimi"
                }),
            }],
            rejected_alternatives: vec![HelpRejectedAlternative {
                tool: "search".into(),
                reason: "Operations work needs system state, not just text retrieval.".into(),
            }],
            recovery_steps: vec!["Standing freshness is the structural daemon's job, and arming it sits above the generic authority floor. Without it, check am_i_stale on your working set and refresh with ingest {mode: \"refresh\"} from the brain's exact declared root.".into()],
            next_step: Some("Run health first, then inspect daemon status and alerts.".into()),
            why: "Operations mode should start from runtime health signals rather than content search.".into(),
            confidence: 0.84,
            canonical_name: None,
            aliases: vec![],
        },
        HelpStage::Handoff => {
            let resume = input.intent.as_deref().unwrap_or_default().to_lowercase().contains("resume");
            if resume {
                HelpGuidance {
                    decision_type: "sequence".into(),
                    recommended_tools: vec![
                        "north".into(),
                        "trail_list".into(),
                        "coverage_session".into(),
                    ],
                    recommended_sequence: vec![
                        HelpSequenceStep {
                            tool: "north".into(),
                            reason: "Start from the task: north carries the durable memory earlier agents left, with each claim's age and author.".into(),
                        },
                        HelpSequenceStep {
                            tool: "trail_list".into(),
                            reason: "Find the saved investigation trails and what they covered.".into(),
                        },
                        HelpSequenceStep {
                            tool: "coverage_session".into(),
                            reason: "Check what has and has not been inspected in this session.".into(),
                        },
                    ],
                    required_inputs: vec!["task".into()],
                    missing_context: vec![],
                    minimal_calls: vec![HelpMinimalCall {
                        tool: "north".into(),
                        arguments: json!({
                            "agent_id": "jimi",
                            "task": input
                                .intent
                                .clone()
                                .unwrap_or_else(|| "resume the previous investigation".into())
                        }),
                    }],
                    rejected_alternatives: vec![HelpRejectedAlternative {
                        tool: "search".into(),
                        reason: "Do not restart a saved investigation as a blind fresh search.".into(),
                    }],
                    recovery_steps: vec![
                        "Rehydrating a saved trail into the live graph sits above the generic authority floor; read what it recorded through trail_list and north instead.".into(),
                        "If there is no saved trail yet, use trail_save before ending the current session.".into(),
                    ],
                    next_step: Some("Run north with the task you are resuming, then open the trail it points to.".into()),
                    why: "Handoff or resume mode should read back the investigation state instead of rebuilding it.".into(),
                    confidence: 0.82,
                    canonical_name: None,
                    aliases: vec![],
                }
            } else {
                HelpGuidance {
                    decision_type: "sequence".into(),
                    recommended_tools: vec!["trail_save".into(), "coverage_session".into()],
                    recommended_sequence: vec![
                        HelpSequenceStep {
                            tool: "trail_save".into(),
                            reason: "Persist the investigation state before handing off or closing the session.".into(),
                        },
                        HelpSequenceStep {
                            tool: "coverage_session".into(),
                            reason: "Capture what was covered and what remains unread.".into(),
                        },
                    ],
                    required_inputs: vec!["title".into()],
                    missing_context: vec![],
                    minimal_calls: vec![HelpMinimalCall {
                        tool: "trail_save".into(),
                        arguments: json!({
                            "agent_id": "jimi",
                            "title": "Auth flow investigation"
                        }),
                    }],
                    rejected_alternatives: vec![HelpRejectedAlternative {
                        tool: "search".into(),
                        reason: "Search does not preserve continuity or handoff state.".into(),
                    }],
                    recovery_steps: vec!["If a finding must outlive the session, memorize it: north reads it back for the next agent.".into()],
                    next_step: Some("Save the trail before handing the work to another agent or session.".into()),
                    why: "Handoff mode should preserve continuity explicitly instead of trusting chat history alone.".into(),
                    confidence: 0.83,
                    canonical_name: None,
                    aliases: vec![],
                }
            }
        }
        HelpStage::Find => {
            let lowered = intent.to_lowercase();
            let (tool, reason, reject_tool, reject_reason) = if lowered.contains("regex")
                || lowered.contains("exact text")
                || lowered.contains("symbol")
                || lowered.contains("identifier")
            {
                (
                    "search",
                    "The query already sounds textual or regex-shaped.",
                    "seek",
                    "Seek is for purpose-first discovery when exact text is not known.",
                )
            } else if lowered.contains("file")
                || lowered.contains("filename")
                || lowered.contains("path")
                || lowered.contains("glob")
            {
                (
                    "glob",
                    "The task is mainly about narrowing a file set by path shape.",
                    "search",
                    "Search is weaker when the problem is path-pattern selection, not content.",
                )
            } else if lowered.contains("flow")
                || lowered.contains("subsystem")
                || lowered.contains("neighborhood")
                || lowered.contains("connected")
            {
                (
                    "activate",
                    "The request sounds relational or subsystem-oriented.",
                    "search",
                    "Text search will miss connected context when the problem is neighborhood-driven.",
                )
            } else {
                (
                    "seek",
                    "The goal is known but the location is not.",
                    "search",
                    "Search assumes exact text, while this is purpose-first retrieval.",
                )
            };
            let arguments = match tool {
                "search" => json!({
                    "agent_id": "jimi",
                    "query": if intent.is_empty() { "retry backoff" } else { intent },
                    "mode": "literal"
                }),
                "glob" => json!({
                    "agent_id": "jimi",
                    "pattern": "**/*.rs",
                    "scope": input.path.clone().unwrap_or_else(|| "src".into())
                }),
                "activate" => json!({
                    "agent_id": "jimi",
                    "query": if intent.is_empty() { "authentication flow" } else { intent }
                }),
                _ => json!({
                    "agent_id": "jimi",
                    "query": if intent.is_empty() { "where retry backoff is decided" } else { intent }
                }),
            };

            HelpGuidance {
                decision_type: if workflow { "sequence" } else { "tool" }.into(),
                recommended_tools: vec![tool.into()],
                recommended_sequence: vec![
                    HelpSequenceStep {
                        tool: tool.into(),
                        reason: reason.into(),
                    },
                    HelpSequenceStep {
                        tool: if tool == "glob" { "batch_view" } else { "view" }.into(),
                        reason: "Open the strongest result after retrieval narrows the surface."
                            .into(),
                    },
                ],
                required_inputs: vec![if tool == "glob" { "pattern" } else { "query" }.into()],
                missing_context: if intent.is_empty() {
                    vec!["intent or query".into()]
                } else {
                    Vec::new()
                },
                minimal_calls: vec![HelpMinimalCall {
                    tool: tool.into(),
                    arguments,
                }],
                rejected_alternatives: vec![HelpRejectedAlternative {
                    tool: reject_tool.into(),
                    reason: reject_reason.into(),
                }],
                recovery_steps: vec!["If the first retrieval is too broad, tighten scope/path or switch to the rejected alternative only when the retrieval shape changes.".into()],
                next_step: Some(format!("Run `{}` first, then open the strongest result.", tool)),
                why: "Find mode should choose retrieval shape from the kind of uncertainty: text, path, purpose, or connected neighborhood.".into(),
                confidence: 0.89,
                canonical_name: None,
                aliases: vec![],
            }
        }
    }
}

fn recovery_guidance(input: &HelpInput) -> HelpGuidance {
    let error_text = input.error_text.clone().unwrap_or_default();
    let lowered = error_text.to_lowercase();

    if lowered.contains("node not found")
        || lowered.contains("no valid node ids found")
        || (lowered.contains("not found")
            && matches!(
                input.tool_name.as_deref(),
                Some(
                    "impact" | "predict" | "counterfactual" | "why" | "fingerprint" | "type_trace"
                )
            ))
    {
        return HelpGuidance {
            decision_type: "recovery".into(),
            recommended_tools: vec!["search".into(), "seek".into()],
            recommended_sequence: vec![
                HelpSequenceStep {
                    tool: "search".into(),
                    reason: "Recover the exact node_id or file identity when you already know the text or path shape.".into(),
                },
                HelpSequenceStep {
                    tool: "seek".into(),
                    reason: "Recover the seam by purpose when you do not know the exact node identifier.".into(),
                },
            ],
            required_inputs: vec!["node_id".into()],
            missing_context: vec!["A canonical node_id present in the current graph".into()],
            minimal_calls: vec![HelpMinimalCall {
                tool: "search".into(),
                arguments: json!({
                    "agent_id": "jimi",
                    "query": "file::src/example.rs",
                    "mode": "literal"
                }),
            }],
            rejected_alternatives: vec![HelpRejectedAlternative {
                tool: "blind retry".into(),
                reason: "Retrying the same analysis tool will fail again until the target resolves to a live node.".into(),
            }],
            recovery_steps: vec![
                "Recover the canonical node identity first.".into(),
                "Retry the original analysis tool only after the graph resolves the node_id.".into(),
            ],
            next_step: Some("Recover the canonical node_id with search or seek, then retry the analysis tool.".into()),
            why: "Node-scoped tools need a live graph identity before they can reason over change or causality.".into(),
            confidence: 0.88,
            canonical_name: input.tool_name.clone(),
            aliases: vec![],
        };
    }

    if lowered.contains("missing field")
        || lowered.contains("missing required")
        || lowered.contains("invalid type")
        || lowered.contains("unknown field")
        || lowered.contains("unknown argument")
    {
        let tool_name = input.tool_name.as_deref().unwrap_or("help");
        let entry = catalog_entry(tool_name);
        let required_inputs = entry
            .as_ref()
            .map(|entry| {
                entry
                    .params
                    .iter()
                    .filter(|param| param.required)
                    .map(|param| param.name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec!["tool_name".into()]);
        let minimal_calls = entry
            .as_ref()
            .map(|entry| {
                vec![HelpMinimalCall {
                    tool: entry.name.clone(),
                    arguments: entry.minimal_arguments.clone(),
                }]
            })
            .unwrap_or_default();

        return HelpGuidance {
            decision_type: "recovery".into(),
            recommended_tools: entry
                .as_ref()
                .map(|entry| vec![entry.name.clone()])
                .unwrap_or_else(|| vec!["help".into()]),
            recommended_sequence: vec![HelpSequenceStep {
                tool: entry
                    .as_ref()
                    .map(|entry| entry.name.clone())
                    .unwrap_or_else(|| "help".into()),
                reason: "Retry with the minimal valid contract instead of reformulating blindly.".into(),
            }],
            required_inputs,
            missing_context: vec!["One or more required inputs are missing or malformed.".into()],
            minimal_calls,
            rejected_alternatives: vec![HelpRejectedAlternative {
                tool: "blind retry".into(),
                reason: "Do not repeat the same malformed call when the contract already tells you what is missing.".into(),
            }],
            recovery_steps: vec![
                "Read the required inputs and default knobs.".into(),
                "Retry with the canonical minimal call first, then widen only if needed.".into(),
            ],
            next_step: Some("Retry with the minimal valid call.".into()),
            why: "Schema or input errors should route you back to the live contract immediately.".into(),
            confidence: 0.91,
            canonical_name: entry.as_ref().map(|entry| entry.name.clone()),
            aliases: entry
                .as_ref()
                .map(|entry| entry.aliases.clone())
                .unwrap_or_default(),
        };
    }

    if lowered.contains("path") || lowered.contains("no such file") || lowered.contains("scope") {
        return HelpGuidance {
            decision_type: "recovery".into(),
            recommended_tools: vec!["glob".into(), "health".into()],
            recommended_sequence: vec![
                HelpSequenceStep {
                    tool: "glob".into(),
                    reason: "Find the path the graph actually knows by pattern instead of guessing it again.".into(),
                },
                HelpSequenceStep {
                    tool: "health".into(),
                    reason: "Read the graph's roots when the path may sit outside what this brain covers.".into(),
                },
            ],
            required_inputs: vec!["path".into()],
            missing_context: vec!["A canonical path or scope".into()],
            minimal_calls: vec![HelpMinimalCall {
                tool: "glob".into(),
                arguments: json!({
                    "agent_id": "jimi",
                    "pattern": "**/*"
                }),
            }],
            rejected_alternatives: vec![HelpRejectedAlternative {
                tool: "search".into(),
                reason: "Do not keep searching until the path or scope is canonicalized.".into(),
            }],
            recovery_steps: vec!["Fix the path/scope first, then re-run the narrower tool.".into()],
            next_step: Some("Find the canonical path with glob, then narrow again.".into()),
            why: "Path and scope failures usually mean the tool choice is fine but the target is not canonical yet.".into(),
            confidence: 0.83,
            canonical_name: None,
            aliases: vec![],
        };
    }

    // An error an m1nd verb returned is m1nd's own refusal, not a failure in the
    // caller's program: route it to that verb's contract. `trace` stays the
    // route for failure text the agent brings from its own code.
    if let Some(entry) = (!error_text.is_empty())
        .then(|| input.tool_name.as_deref().and_then(catalog_entry))
        .flatten()
    {
        return HelpGuidance {
            decision_type: "recovery".into(),
            recommended_tools: vec!["help".into()],
            recommended_sequence: vec![HelpSequenceStep {
                tool: "help".into(),
                reason: format!(
                    "Read `{}`'s contract: required inputs, defaults, and what its refusals mean.",
                    entry.name
                ),
            }],
            required_inputs: vec!["tool_name".into()],
            missing_context: vec![],
            minimal_calls: vec![HelpMinimalCall {
                tool: "help".into(),
                arguments: json!({
                    "agent_id": "jimi",
                    "tool_name": entry.name
                }),
            }],
            rejected_alternatives: vec![HelpRejectedAlternative {
                tool: "blind retry".into(),
                reason: "Repeating the same call returns the same refusal.".into(),
            }],
            recovery_steps: vec![
                "Read the refusal text first: it names the field, state, or authority that is missing.".into(),
                "Retry only after changing what the refusal names.".into(),
            ],
            next_step: Some(format!(
                "Open help(tool_name=\"{}\") and fix what the refusal names before retrying.",
                entry.name
            )),
            why: "This error came from an m1nd verb, so the verb's own contract is the shortest path to a call that succeeds.".into(),
            confidence: 0.8,
            canonical_name: Some(entry.name.clone()),
            aliases: entry.aliases.clone(),
        };
    }

    if !error_text.is_empty() {
        return HelpGuidance {
            decision_type: "recovery".into(),
            recommended_tools: vec!["trace".into(), "help".into()],
            recommended_sequence: vec![
                HelpSequenceStep {
                    tool: "trace".into(),
                    reason: "Turn the observed failure into the strongest next file or seam.".into(),
                },
                HelpSequenceStep {
                    tool: "help".into(),
                    reason: "Use tool-specific help only after the failure is mapped to the right seam.".into(),
                },
            ],
            required_inputs: vec!["error_text".into()],
            missing_context: vec![],
            minimal_calls: vec![HelpMinimalCall {
                tool: "trace".into(),
                arguments: json!({
                    "agent_id": "jimi",
                    "error_text": error_text
                }),
            }],
            rejected_alternatives: vec![HelpRejectedAlternative {
                tool: "search".into(),
                reason: "Do not translate runtime failure back into blind keyword search until trace has routed it.".into(),
            }],
            recovery_steps: vec!["If trace returns a broad seam, open it and then narrow with search or impact.".into()],
            next_step: Some("Trace the failure text first.".into()),
            why: "Observed failures should route through trace before any generic retry strategy.".into(),
            confidence: 0.87,
            canonical_name: None,
            aliases: vec![],
        };
    }

    HelpGuidance {
        decision_type: "recovery".into(),
        recommended_tools: vec!["help".into()],
        recommended_sequence: vec![HelpSequenceStep {
            tool: "help".into(),
            reason: "Use stage or intent to route the next action when the failure is still underspecified.".into(),
        }],
        required_inputs: vec!["stage or intent".into()],
        missing_context: vec!["Observed error or clearer intent".into()],
        minimal_calls: vec![HelpMinimalCall {
            tool: "help".into(),
            arguments: json!({
                "agent_id": "jimi",
                "stage": "find",
                "intent": "where retry backoff is decided"
            }),
        }],
        rejected_alternatives: vec![],
        recovery_steps: vec!["Give help either the tool name, the stage, the intent, or the error text.".into()],
        next_step: Some("Provide stage, intent, or error text for a tighter route.".into()),
        why: "Recovery mode needs at least one strong signal: tool, stage, intent, or error.".into(),
        confidence: 0.52,
        canonical_name: None,
        aliases: vec![],
    }
}

fn infer_stage_from_intent(intent: Option<&str>) -> Option<HelpStage> {
    let lowered = intent?.to_lowercase();
    if lowered.contains("error") || lowered.contains("trace") || lowered.contains("stack") {
        Some(HelpStage::Diagnose)
    } else if lowered.contains("spec")
        || lowered.contains("doc")
        || lowered.contains("pdf")
        || lowered.contains("light")
    {
        Some(HelpStage::Ground)
    } else if lowered.contains("plan") || lowered.contains("blast") || lowered.contains("impact") {
        Some(HelpStage::Plan)
    } else if lowered.contains("edit")
        || lowered.contains("write")
        || lowered.contains("patch")
        || lowered.contains("batch")
    {
        Some(HelpStage::Edit)
    } else if lowered.contains("review") || lowered.contains("diff") || lowered.contains("history")
    {
        Some(HelpStage::Review)
    } else if lowered.contains("operate")
        || lowered.contains("monitor")
        || lowered.contains("daemon")
        || lowered.contains("alert")
    {
        Some(HelpStage::Operate)
    } else if lowered.contains("handoff")
        || lowered.contains("resume")
        || lowered.contains("continue")
        || lowered.contains("trail")
    {
        Some(HelpStage::Handoff)
    } else if lowered.contains("orient") || lowered.contains("understand the repo") {
        Some(HelpStage::Orient)
    } else {
        Some(HelpStage::Find)
    }
}

fn normalize_recent_tools(recent_tools: &[String]) -> BTreeSet<String> {
    recent_tools
        .iter()
        .map(|tool| {
            tool.trim()
                .trim_start_matches("m1nd.")
                .trim_start_matches("m1nd_")
        })
        .filter(|tool| !tool.is_empty())
        .map(str::to_string)
        .collect()
}

fn overview_cards(input: &HelpInput) -> Vec<OverviewCard> {
    let task = input
        .intent
        .clone()
        .unwrap_or_else(|| "understand this repository before editing".into());
    vec![
        OverviewCard {
            title: "Need repo orientation",
            why: "One call returns trust, focus nodes for your task, durable memory, and their source.",
            tool: "north",
            arguments: json!({
                "agent_id": "jimi",
                "task": task
            }),
            reject_tool: "search",
            reject_reason: "Text search is too early when the repo surface is still unknown.",
        },
        OverviewCard {
            title: "Know the purpose, not the location",
            why: "Route by intent before opening files.",
            tool: "seek",
            arguments: json!({
                "agent_id": "jimi",
                "query": "where retry backoff is decided"
            }),
            reject_tool: "search",
            reject_reason: "Search assumes exact text and misses intent-first discovery.",
        },
        OverviewCard {
            title: "Have docs, spec, wiki, or PDF",
            why: "Resolve a document the graph already holds, then bind it back to code.",
            tool: "document_resolve",
            arguments: json!({
                "agent_id": "jimi",
                "path": input.path.clone().unwrap_or_else(|| "docs/spec.md".into())
            }),
            reject_tool: "search",
            reject_reason: "Searching a document blind loses its bindings and drift detection.",
        },
        OverviewCard {
            title: "Have runtime failure text",
            why: "Turn the error into the best next seam before manual inspection.",
            tool: "trace",
            arguments: json!({
                "agent_id": "jimi",
                "error_text": "TypeError: undefined is not a function"
            }),
            reject_tool: "search",
            reject_reason: "Trace is a better first move when the failure text is already available.",
        },
        OverviewCard {
            title: "Need to plan a risky change",
            why: "Prove the seam before any write surface executes.",
            tool: "validate_plan",
            arguments: json!({
                "agent_id": "jimi",
                "plan": [{
                    "file": "src/example.rs",
                    "action": "modify"
                }]
            }),
            reject_tool: "blind edit",
            reject_reason: "Writing is too early until the change stops proving and becomes safe to execute.",
        },
        OverviewCard {
            title: "Need deep architecture or risk lens",
            why: "Start from module-level risk concentration, then use twins, taint_trace, or refactor_plan for duplication, trust-boundary, and extraction seams.",
            tool: "panoramic",
            arguments: json!({
                "agent_id": "jimi"
            }),
            reject_tool: "search",
            reject_reason: "Raw text search cannot surface risk concentration, taint paths, structural twins, or refactor communities.",
        },
        OverviewCard {
            title: "Need continuity or handoff",
            why: "Persist investigation state explicitly; north reads memory back for the next session.",
            tool: "trail_save",
            arguments: json!({
                "agent_id": "jimi",
                "title": "Auth flow investigation"
            }),
            reject_tool: "search",
            reject_reason: "Search does not preserve continuity across sessions or agents.",
        },
    ]
}

fn render_tool(entry: &HelpCatalogEntry, guidance: &HelpGuidance, compact: bool) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "{} {}  {}\n",
        entry.glyph,
        entry.category.to_uppercase(),
        entry.name
    ));
    out.push_str(&format!("canonical: {}", entry.name));
    if !entry.aliases.is_empty() {
        out.push_str(&format!("  aliases: {}\n", entry.aliases.join(", ")));
    } else {
        out.push('\n');
    }
    out.push('\n');

    render_section_title(&mut out, "USE THIS NOW");
    out.push_str(&format!("  {}\n\n", entry.name));

    render_section_title(&mut out, "WHY");
    out.push_str(&format!("  - {}\n", entry.one_liner));
    if !entry.returns.is_empty() {
        out.push_str(&format!("  - Returns: {}\n", entry.returns));
    }
    for line in personality::when_to_use(&entry.name)
        .iter()
        .take(if compact { 1 } else { 2 })
    {
        out.push_str(&format!("  - {}\n", line));
    }
    out.push('\n');

    render_section_title(&mut out, "REQUIRED INPUT");
    let required: Vec<&HelpParamSpec> =
        entry.params.iter().filter(|param| param.required).collect();
    if required.is_empty() {
        out.push_str("  - no required inputs beyond the call itself\n");
    } else {
        for param in required {
            out.push_str(&format!("  - {} — {}\n", param.name, param.description));
        }
    }
    let optional: Vec<String> = entry
        .params
        .iter()
        .filter(|param| !param.required)
        .map(|param| param.name.clone())
        .collect();
    if !optional.is_empty() {
        let max = if compact { 4 } else { 8 };
        out.push_str(&format!(
            "  - optional knobs: {}\n",
            optional
                .into_iter()
                .take(max)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    out.push('\n');

    render_section_title(&mut out, "MINIMAL CALL");
    if let Some(call) = guidance.minimal_calls.first() {
        out.push_str(&format!(
            "  {}({})\n\n",
            call.tool,
            render_inline_json(&call.arguments)
        ));
    }

    render_section_title(&mut out, "DO NOT USE IF");
    let avoid = personality::avoid_when(&entry.name);
    if avoid.is_empty() && guidance.rejected_alternatives.is_empty() {
        out.push_str(
            "  - the retrieval or reasoning shape you need is fundamentally different\n\n",
        );
    } else {
        for line in avoid.iter().take(if compact { 1 } else { 2 }) {
            out.push_str(&format!("  - {}\n", line));
        }
        for alternative in guidance
            .rejected_alternatives
            .iter()
            .take(if compact { 1 } else { 2 })
        {
            out.push_str(&format!(
                "  - do not substitute with `{}`: {}\n",
                alternative.tool, alternative.reason
            ));
        }
        out.push('\n');
    }

    render_section_title(&mut out, "IF THIS FAILS");
    for step in guidance
        .recovery_steps
        .iter()
        .take(if compact { 2 } else { 4 })
    {
        out.push_str(&format!("  - {}\n", step));
    }
    out.push('\n');

    render_section_title(&mut out, "NEXT");
    let next = if entry.next.is_empty() {
        vec!["help".into()]
    } else {
        entry.next.clone()
    };
    for tool in next.iter().take(if compact { 2 } else { 4 }) {
        out.push_str(&format!("  - {}\n", tool));
    }
    out.push('\n');
    out
}

fn render_overview(
    total_tools: usize,
    cards: &[OverviewCard],
    compact: bool,
    show_temponizer: bool,
) -> String {
    let mut out = String::new();
    out.push_str("  m1nd help\n");
    out.push_str(&format!(
        "  {} live tools. route by stage, intent, or failure.\n",
        total_tools
    ));
    out.push_str("  structure · change · docs · operations · continuity\n\n");

    render_section_title(&mut out, "30-SECOND MAP");
    for card in cards.iter().take(if compact { 4 } else { cards.len() }) {
        out.push_str(&format!(
            "  - {} -> {} — {}\n",
            card.title, card.tool, card.why
        ));
    }
    out.push('\n');

    render_section_title(&mut out, "HOW TO ASK");
    out.push_str("  - help(tool_name=\"activate\") for exact tool doctrine\n");
    out.push_str("  - help(stage=\"plan\", intent=\"validate a risky change\") for a route\n");
    out.push_str("  - help(error_text=\"...\") for failure recovery\n");
    out.push_str("  - help(mode=\"workflow\", stage=\"edit\") for a full sequence\n");

    if show_temponizer {
        out.push('\n');
        render_section_title(&mut out, "TEMPONIZER");
        out.push_str("  - you estimate in human-time but execute much faster; fill the recovered time with more deliverables, not padding.\n");
    }

    out.push('\n');
    out
}

fn render_guidance(title: &str, guidance: &HelpGuidance, compact: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!("  help {}\n\n", title));

    render_section_title(&mut out, "USE THIS NOW");
    for tool in guidance
        .recommended_tools
        .iter()
        .take(if compact { 2 } else { 4 })
    {
        out.push_str(&format!("  - {}\n", tool));
    }
    out.push('\n');

    render_section_title(&mut out, "WHY");
    out.push_str(&format!("  - {}\n\n", guidance.why));

    render_section_title(&mut out, "REQUIRED INPUT");
    if guidance.required_inputs.is_empty() {
        out.push_str("  - no extra input beyond the current state\n");
    } else {
        out.push_str(&format!("  - {}\n", guidance.required_inputs.join(", ")));
    }
    if !guidance.missing_context.is_empty() {
        out.push_str(&format!(
            "  - still missing: {}\n",
            guidance.missing_context.join(", ")
        ));
    }
    out.push('\n');

    render_section_title(&mut out, "MINIMAL CALL");
    for call in guidance
        .minimal_calls
        .iter()
        .take(if compact { 1 } else { 3 })
    {
        out.push_str(&format!(
            "  {}({})\n",
            call.tool,
            render_inline_json(&call.arguments)
        ));
    }
    out.push('\n');

    render_section_title(&mut out, "DO NOT USE IF");
    if guidance.rejected_alternatives.is_empty() {
        out.push_str(
            "  - the uncertainty shape has changed and now needs a different first tool\n",
        );
    } else {
        for alternative in guidance
            .rejected_alternatives
            .iter()
            .take(if compact { 1 } else { 3 })
        {
            out.push_str(&format!(
                "  - `{}`: {}\n",
                alternative.tool, alternative.reason
            ));
        }
    }
    out.push('\n');

    render_section_title(&mut out, "IF THIS FAILS");
    for step in guidance
        .recovery_steps
        .iter()
        .take(if compact { 2 } else { 4 })
    {
        out.push_str(&format!("  - {}\n", step));
    }
    out.push('\n');

    render_section_title(&mut out, "NEXT");
    for step in guidance
        .recommended_sequence
        .iter()
        .take(if compact { 2 } else { 4 })
    {
        out.push_str(&format!("  - {} — {}\n", step.tool, step.reason));
    }
    if let Some(next_step) = &guidance.next_step {
        out.push_str(&format!("  - {}\n", next_step));
    }
    out.push('\n');
    out
}

fn render_unknown_tool(query: &str, suggestions: &[String]) -> String {
    let mut out = String::new();
    out.push_str("  help recovery\n\n");
    out.push_str(&format!("unknown tool: {}\n", query));
    if suggestions.is_empty() {
        out.push_str(
            "no similar live tool names found. Use stage or intent instead of guessing.\n",
        );
    } else {
        out.push_str(&format!("did you mean: {}\n", suggestions.join(", ")));
    }
    out.push('\n');
    out
}

fn render_section_title(out: &mut String, title: &str) {
    out.push_str(&format!("{}\n", title));
}

fn render_inline_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".into())
}

pub(crate) fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];

    for (i, ch_a) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, ch_b) in b.iter().enumerate() {
            let cost = usize::from(ch_a != ch_b);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        prev.clone_from(&curr);
    }

    prev[b.len()]
}

/// The oracle behind the honest-front-door battery: can the caller reading an
/// agent-facing string actually call the verb it names?
///
/// The caller in question is a plain generic MCP/REST client — no authority
/// lease. The answer is DERIVED from the M1ND-10 action catalog, the same floor
/// table `server::enforce_generic_action_policy` enforces, never from a
/// hand-kept list: a verb is reachable when at least one of its semantic actions
/// sits at ORDINARY, or is one of the action-keyed generic admissions
/// (`GENERIC_A2_LOCAL_ADMITTED_ACTIONS` — today only the declared-root refresh).
/// An exact call (`tool` + `arguments`) is judged by the gate itself.
#[cfg(test)]
pub(crate) mod front_door_oracle {
    use crate::protocol::layers::HelpGuidance;
    use serde_json::Value;
    use std::collections::BTreeSet;

    /// Every verb the registry carries — the vocabulary a string can name.
    pub(crate) fn registry() -> BTreeSet<String> {
        super::live_tool_names().into_iter().collect()
    }

    /// Action -> authority floor, read once from the canonical M1ND-10 catalog.
    fn floor_table() -> &'static std::collections::BTreeMap<String, m1nd_control::AuthorityFloor> {
        static TABLE: std::sync::OnceLock<
            std::collections::BTreeMap<String, m1nd_control::AuthorityFloor>,
        > = std::sync::OnceLock::new();
        TABLE.get_or_init(|| {
            m1nd_control::m1nd10_action_catalog()
                .expect("M1ND-10 catalog validates")
                .entries
                .into_iter()
                .map(|entry| (entry.action.as_str().to_string(), entry.authority_floor))
                .collect()
        })
    }

    /// Whether a lease-less generic caller reaches at least one branch of `verb`.
    pub(crate) fn generic_caller_reaches(verb: &str) -> bool {
        if crate::mission_service_transport::legacy_mutation_refusal(verb).is_some() {
            return false;
        }
        let Some(actions) = crate::action_routes::possible_mcp_actions(verb) else {
            return false;
        };
        actions.into_iter().any(|action| {
            crate::action_consumers::GENERIC_A2_LOCAL_ADMITTED_ACTIONS.contains(&action)
                || floor_table().get(action) == Some(&m1nd_control::AuthorityFloor::Ordinary)
        })
    }

    fn floors(verb: &str) -> String {
        crate::action_routes::possible_mcp_authority_floors(verb)
            .map(|floors| {
                floors
                    .into_iter()
                    .map(|floor| format!("{floor:?}"))
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .unwrap_or_else(|error| format!("unresolved: {error}"))
    }

    /// Collects every place an agent-facing surface names a verb its reader is
    /// refused, then fails once with the whole list.
    pub(crate) struct FrontDoorAudit {
        registry: BTreeSet<String>,
        reaches: std::collections::HashMap<String, bool>,
        gate: std::collections::HashMap<String, Option<String>>,
        offences: BTreeSet<String>,
    }

    impl FrontDoorAudit {
        pub(crate) fn new() -> Self {
            Self {
                registry: registry(),
                reaches: std::collections::HashMap::new(),
                gate: std::collections::HashMap::new(),
                offences: BTreeSet::new(),
            }
        }

        fn reaches(&mut self, verb: &str) -> bool {
            *self
                .reaches
                .entry(verb.to_string())
                .or_insert_with(|| generic_caller_reaches(verb))
        }

        /// A structured verb slot (a `tool` field, a next-list entry).
        ///
        /// `subject` is the verb a page is ABOUT: `help(tool_name=X)` may name X
        /// even when X is policy-disabled — the page says so — but it may not
        /// route its reader on to another refused verb.
        pub(crate) fn verb(&mut self, surface: &str, verb: &str, subject: Option<&str>) {
            let bare = verb
                .trim()
                .strip_prefix("m1nd.")
                .or_else(|| verb.trim().strip_prefix("m1nd_"))
                .unwrap_or(verb.trim());
            if Some(bare) == subject || !self.registry.contains(bare) {
                return;
            }
            if !self.reaches(bare) {
                self.offences.insert(format!(
                    "{surface}: names `{bare}` (authority floor {}), which a generic caller cannot call",
                    floors(bare)
                ));
            }
        }

        /// Free text: every identifier that is a registry verb counts as named.
        pub(crate) fn text(&mut self, surface: &str, text: &str, subject: Option<&str>) {
            for token in text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')) {
                if !token.is_empty() {
                    self.verb(surface, token, subject);
                }
            }
        }

        /// An exact example call, judged by the generic gate itself.
        pub(crate) fn call(
            &mut self,
            surface: &str,
            tool: &str,
            arguments: &Value,
            subject: Option<&str>,
        ) {
            if !self.registry.contains(tool) {
                return;
            }
            // The page of a verb with NO reachable branch may show its own
            // call: it already carries the POLICY-DISABLED mark. A verb with an
            // open branch must demonstrate that branch. The typed G2/G3
            // consumers are intercepted before the generic gate and refuse on
            // their own typed terms, so the gate is not their judge.
            if Some(tool) == subject
                && (!self.reaches(tool) || crate::server::TYPED_CONSUMER_TOOLS.contains(&tool))
            {
                return;
            }
            let refusal = self
                .gate
                .entry(format!("{tool}{arguments}"))
                .or_insert_with(|| {
                    crate::server::enforce_generic_action_policy(tool, arguments)
                        .err()
                        .map(|error| error.to_string())
                })
                .clone();
            if let Some(error) = refusal {
                self.offences.insert(format!(
                    "{surface}: teaches the call {tool}({arguments}), which the generic gate refuses: {error}"
                ));
            }
        }

        pub(crate) fn guidance(
            &mut self,
            surface: &str,
            guidance: &HelpGuidance,
            subject: Option<&str>,
        ) {
            for tool in &guidance.recommended_tools {
                self.verb(&format!("{surface}.recommended_tools"), tool, subject);
            }
            for step in &guidance.recommended_sequence {
                let at = format!("{surface}.recommended_sequence");
                self.verb(&at, &step.tool, subject);
                self.text(&at, &step.reason, subject);
            }
            for call in &guidance.minimal_calls {
                self.call(
                    &format!("{surface}.minimal_calls"),
                    &call.tool,
                    &call.arguments,
                    subject,
                );
            }
            for alternative in &guidance.rejected_alternatives {
                let at = format!("{surface}.rejected_alternatives");
                self.verb(&at, &alternative.tool, subject);
                self.text(&at, &alternative.reason, subject);
            }
            for line in guidance
                .recovery_steps
                .iter()
                .chain(&guidance.missing_context)
                .chain(&guidance.required_inputs)
            {
                self.text(surface, line, subject);
            }
            if let Some(next) = &guidance.next_step {
                self.text(&format!("{surface}.next_step"), next, subject);
            }
            // A tool page's `why` is the subject's own description, not a route.
            if subject.is_none() {
                self.text(&format!("{surface}.why"), &guidance.why, subject);
            }
        }

        pub(crate) fn assert_clean(self, what: &str) {
            assert!(
                self.offences.is_empty(),
                "{what} must name only verbs a generic caller can call — the authority \
                 floor table refuses every one below ({} offences):\n  {}",
                self.offences.len(),
                self.offences.into_iter().collect::<Vec<_>>().join("\n  ")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::layers::{HelpMode, HelpRender};
    use std::collections::{BTreeSet, HashMap};

    fn base_input() -> HelpInput {
        HelpInput {
            agent_id: "jimi".into(),
            tool_name: None,
            mode: None,
            intent: None,
            stage: None,
            path: None,
            error_text: None,
            recent_tools: vec![],
            max_suggestions: None,
            render: Some(HelpRender::Full),
        }
    }

    #[test]
    fn catalog_covers_live_runtime_surface() {
        let schema_names: BTreeSet<String> = schema_tools()
            .into_iter()
            .map(|schema| schema.name)
            .collect();
        let catalog_names: BTreeSet<String> = catalog_entries()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(
            catalog_names, schema_names,
            "help catalog must cover every tool exposed by tool_schemas()"
        );
    }

    #[test]
    fn catalog_minimal_arguments_validate_against_live_schema() {
        let catalog_by_name: HashMap<String, HelpCatalogEntry> = catalog_entries()
            .into_iter()
            .map(|entry| (entry.name.clone(), entry))
            .collect();

        for schema in schema_tools() {
            let entry = catalog_by_name
                .get(&schema.name)
                .unwrap_or_else(|| panic!("missing help catalog entry for {}", schema.name));
            assert!(
                validate_against_schema(&entry.minimal_arguments, &schema.input_schema),
                "minimal call for {} must validate against the live input schema",
                schema.name
            );
        }
    }

    #[test]
    fn route_mode_prefers_seek_for_purpose_first_queries() {
        let mut input = base_input();
        input.mode = Some(HelpMode::Route);
        input.stage = Some(HelpStage::Find);
        input.intent = Some("where retry backoff is decided".into());

        let guidance = route_guidance(&input, false);
        assert_eq!(
            guidance.recommended_tools.first().map(String::as_str),
            Some("seek")
        );
        assert_eq!(
            guidance
                .minimal_calls
                .first()
                .map(|call| call.tool.as_str()),
            Some("seek")
        );
    }

    #[test]
    fn recovery_mode_prefers_trace_when_error_text_is_present() {
        let mut input = base_input();
        input.mode = Some(HelpMode::Recovery);
        input.error_text = Some("TypeError: undefined is not a function".into());

        let guidance = recovery_guidance(&input);
        assert_eq!(
            guidance.recommended_tools.first().map(String::as_str),
            Some("trace")
        );
        assert_eq!(
            guidance
                .minimal_calls
                .first()
                .map(|call| call.tool.as_str()),
            Some("trace")
        );
    }

    #[test]
    fn recovery_mode_prefers_search_when_node_identity_is_missing() {
        let mut input = base_input();
        input.mode = Some(HelpMode::Recovery);
        input.tool_name = Some("impact".into());
        input.error_text = Some("Node not found: file::src/missing.rs".into());

        let guidance = recovery_guidance(&input);
        assert_eq!(
            guidance.recommended_tools.first().map(String::as_str),
            Some("search")
        );
        assert!(guidance
            .next_step
            .as_deref()
            .unwrap_or_default()
            .contains("canonical node_id"));
    }

    #[test]
    fn runtime_projection_prefers_sequence_head_and_surfaces_missing_context() {
        let projection = runtime_projection_from_guidance(
            Some(&HelpGuidance {
                decision_type: "sequence".into(),
                recommended_tools: vec!["audit".into()],
                recommended_sequence: vec![HelpSequenceStep {
                    tool: "glob".into(),
                    reason: "Open the next likely file.".into(),
                }],
                required_inputs: vec!["path".into()],
                missing_context: vec!["path".into()],
                minimal_calls: vec![],
                rejected_alternatives: vec![],
                recovery_steps: vec![],
                next_step: Some("Run glob next.".into()),
                why: "The path is still missing.".into(),
                confidence: 0.9,
                canonical_name: Some("audit".into()),
                aliases: vec!["m1nd.audit".into()],
            }),
            "blocked",
        );

        assert_eq!(projection.proof_state, "blocked");
        assert_eq!(projection.next_suggested_tool.as_deref(), Some("glob"));
        assert_eq!(projection.next_suggested_target.as_deref(), Some("path"));
        assert_eq!(projection.what_is_missing.as_deref(), Some("path"));
    }

    #[test]
    fn runtime_error_hint_includes_minimal_call_and_next_step() {
        let hint = runtime_error_guidance_hint("impact", "Node not found: file::src/missing.rs");
        assert!(hint.contains("minimal call: search("));
        assert!(hint.contains("next: Recover the canonical node_id"));
    }

    // ---- The honest front door ---------------------------------------------
    //
    // An agent that follows m1nd's own directions must not be walked into a
    // refusal. Every route, card, next list and recovery hint below is checked
    // against the authority floor table (`front_door_oracle`): a verb a generic
    // caller cannot call may not be named, and an example call the generic gate
    // refuses may not be taught.

    use super::front_door_oracle::{registry, FrontDoorAudit};

    const ALL_STAGES: [HelpStage; 9] = [
        HelpStage::Orient,
        HelpStage::Find,
        HelpStage::Ground,
        HelpStage::Diagnose,
        HelpStage::Plan,
        HelpStage::Edit,
        HelpStage::Review,
        HelpStage::Operate,
        HelpStage::Handoff,
    ];

    /// Intents that select every special branch the router has.
    const ROUTE_INTENTS: [Option<&str>; 9] = [
        None,
        Some("understand the repo"),
        Some("ground the light spec doc"),
        Some("resume the saved trail"),
        Some("regex for an exact symbol"),
        Some("narrow the file path glob"),
        Some("connected flow in the subsystem"),
        Some("track changes in the graph"),
        Some("where retry backoff is decided"),
    ];

    const ERROR_TEXTS: [Option<&str>; 9] = [
        None,
        Some("Node not found: file::src/missing.rs"),
        Some("missing field `query`"),
        Some("invalid type: integer `3`, expected a string"),
        Some("unknown field `tool`, expected one of `agent_id`, `tool_name`"),
        Some("path does not exist: docs/spec.md"),
        Some("No such file or directory (os error 2)"),
        Some("scope is outside the graph"),
        Some("refused: no record with that id"),
    ];

    #[test]
    fn overview_cards_name_only_verbs_a_generic_caller_can_call() {
        let mut audit = FrontDoorAudit::new();
        for path in [None, Some("/path/to/project".to_string())] {
            let mut input = base_input();
            input.path = path;
            for card in overview_cards(&input) {
                let at = format!("help() card {:?}", card.title);
                audit.call(&at, card.tool, &card.arguments, None);
                audit.verb(&at, card.reject_tool, None);
                for text in [card.title, card.why, card.reject_reason] {
                    audit.text(&at, text, None);
                }
            }
            let overview = build_overview_resolution(&input, false);
            audit.guidance(
                "help() overview",
                overview.guidance.as_ref().expect("overview guidance"),
                None,
            );
        }
        audit.assert_clean("the help overview cards");
    }

    #[test]
    fn stage_routes_name_only_verbs_a_generic_caller_can_call() {
        let mut audit = FrontDoorAudit::new();
        let stages = ALL_STAGES.iter().copied().map(Some).chain([None]);
        for stage in stages {
            for intent in ROUTE_INTENTS {
                for recent in [
                    vec![],
                    vec!["impact".to_string()],
                    vec!["validate_plan".to_string()],
                ] {
                    for path in [None, Some("src/example.rs".to_string())] {
                        for workflow in [false, true] {
                            let mut input = base_input();
                            input.stage = stage;
                            input.intent = intent.map(str::to_string);
                            input.recent_tools = recent.clone();
                            input.path = path.clone();
                            let guidance = route_guidance(&input, workflow);
                            audit.guidance(
                                &format!(
                                    "help(stage={stage:?}, intent={intent:?}, recent={recent:?}, workflow={workflow})"
                                ),
                                &guidance,
                                None,
                            );
                        }
                    }
                }
            }
        }
        audit.assert_clean("the help stage routes");
    }

    #[test]
    fn recovery_routes_and_runtime_error_hints_name_only_callable_verbs() {
        let mut audit = FrontDoorAudit::new();
        let tools: Vec<Option<String>> = std::iter::once(None)
            .chain(registry().into_iter().map(Some))
            .collect();
        for tool in &tools {
            for error_text in ERROR_TEXTS {
                let mut input = base_input();
                input.mode = Some(HelpMode::Recovery);
                input.tool_name = tool.clone();
                input.error_text = error_text.map(str::to_string);
                let guidance = recovery_guidance(&input);
                audit.guidance(
                    &format!("help(mode=recovery, tool_name={tool:?}, error_text={error_text:?})"),
                    &guidance,
                    tool.as_deref(),
                );
                if let (Some(tool), Some(detail)) = (tool, error_text) {
                    let hint = runtime_error_guidance_hint(tool, detail);
                    let routed = hint.strip_prefix(detail).unwrap_or(&hint);
                    audit.text(
                        &format!("{tool} error hint for {detail:?}"),
                        routed,
                        Some(tool),
                    );
                }
            }
        }
        audit.assert_clean("help recovery routes and runtime error hints");
    }

    #[test]
    fn tool_pages_and_next_lists_name_only_callable_verbs() {
        let mut audit = FrontDoorAudit::new();
        for entry in catalog_entries() {
            let subject = Some(entry.name.as_str());
            let at = format!("help(tool_name={})", entry.name);
            for next in &entry.next {
                audit.verb(&format!("{at}.next"), next, subject);
            }
            let mut input = base_input();
            input.tool_name = Some(entry.name.clone());
            audit.guidance(&at, &tool_guidance(&entry, &input), subject);
            for line in personality::when_to_use(&entry.name)
                .iter()
                .chain(personality::avoid_when(&entry.name))
            {
                audit.text(&at, line, subject);
            }
        }
        for doc in personality::tool_docs() {
            for next in doc.next {
                audit.verb(&format!("ToolDoc({}).next", doc.name), next, Some(doc.name));
            }
        }
        audit.assert_clean("help tool pages and their next lists");
    }

    #[test]
    fn suggest_next_names_only_verbs_a_generic_caller_can_call() {
        let mut audit = FrontDoorAudit::new();
        let names = registry()
            .into_iter()
            .chain(["an_unrouted_verb".to_string()]);
        for name in names {
            for line in personality::suggest_next(&name) {
                audit.text(
                    &format!("_m1nd.suggest_next after {name}"),
                    &line,
                    Some(&name),
                );
            }
        }
        audit.assert_clean("the _m1nd.suggest_next lists");
    }

    #[test]
    fn a_verb_error_routes_to_the_verb_contract_not_to_failure_triage() {
        // `trace` turns the caller's OWN program failure into a next file. An
        // m1nd refusal is not that: routing it to `trace` sends the agent to
        // triage m1nd's error text as if it were their stacktrace.
        let detail = "refused: no record with that id";
        let hint = runtime_error_guidance_hint("receipt_recompute", detail);
        assert!(
            !hint.contains("trace("),
            "an m1nd verb's own error must not be routed to trace: {hint}"
        );
        assert!(
            hint.contains("help(") && hint.contains("receipt_recompute"),
            "an m1nd verb's own error routes to that verb's contract: {hint}"
        );

        // A failure the agent pastes from ITS program still routes to trace.
        let mut input = base_input();
        input.mode = Some(HelpMode::Recovery);
        input.error_text = Some("TypeError: undefined is not a function".into());
        assert_eq!(
            recovery_guidance(&input)
                .recommended_tools
                .first()
                .map(String::as_str),
            Some("trace")
        );
    }
}
