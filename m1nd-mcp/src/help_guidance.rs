use crate::personality::{
    self, gradient_bottom_border, gradient_top_border, ANSI_BOLD, ANSI_CYAN, ANSI_DIM, ANSI_GOLD,
    ANSI_GREEN, ANSI_MAGENTA, ANSI_RED, ANSI_RESET, GLYPH_CONNECTION, GLYPH_DIMENSION, GLYPH_PATH,
    GLYPH_SIGNAL, GLYPH_STRUCTURE,
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
            let one_liner = manual
                .map(|doc| doc.one_liner.to_string())
                .unwrap_or_else(|| schema.description.clone());
            let returns = manual
                .map(|doc| doc.returns.to_string())
                .unwrap_or_else(|| {
                    "Structured output documented by the live runtime schema.".into()
                });
            let next = manual
                .map(|doc| doc.next.iter().map(|next| (*next).to_string()).collect())
                .unwrap_or_else(|| inferred_next_tools(&schema.name));
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

fn schema_tools() -> Vec<SchemaTool> {
    crate::server::tool_schemas()
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
        "document_drift" => vec!["document_bindings".into(), "audit".into()],
        "auto_ingest_start" => vec!["auto_ingest_status".into(), "auto_ingest_tick".into()],
        "auto_ingest_tick" | "auto_ingest_status" => {
            vec!["document_bindings".into(), "audit".into()]
        }
        "daemon_start" => vec!["daemon_status".into(), "alerts_list".into()],
        "daemon_status" => vec!["alerts_list".into(), "audit".into()],
        "alerts_list" => vec!["alerts_ack".into(), "daemon_status".into()],
        "boot_memory" => vec!["trail_save".into(), "trail_resume".into()],
        "heuristics_surface" => vec!["batch_view".into(), "validate_plan".into()],
        "type_trace" => vec!["taint_trace".into(), "validate_plan".into()],
        "metrics" | "persist" => vec!["report".into(), "audit".into()],
        "diagram" => vec!["audit".into(), "panoramic".into()],
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
            .map(|tool| HelpSequenceStep {
                tool: tool.clone(),
                reason: format!("Typical downstream follow-up after `{}`.", entry.name),
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
            "apply_batch",
            "Do not execute a connected edit before the plan has passed structural validation.",
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
    let stage = input
        .stage
        .or_else(|| infer_stage_from_intent(input.intent.as_deref()))
        .unwrap_or(HelpStage::Find);
    let recent = normalize_recent_tools(&input.recent_tools);
    let intent = input.intent.as_deref().unwrap_or_default();
    let path = input.path.clone();

    match stage {
        HelpStage::Orient => HelpGuidance {
            decision_type: if workflow { "sequence" } else { "tool" }.into(),
            recommended_tools: vec!["audit".into()],
            recommended_sequence: vec![
                HelpSequenceStep {
                    tool: "audit".into(),
                    reason: "Get a one-call structural orientation pass before narrower retrieval."
                        .into(),
                },
                HelpSequenceStep {
                    tool: "batch_view".into(),
                    reason: "Open the strongest files or findings after the audit narrows the surface."
                        .into(),
                },
            ],
            required_inputs: vec!["path".into()],
            missing_context: path
                .as_ref()
                .map(|_| Vec::new())
                .unwrap_or_else(|| vec!["path".into()]),
            minimal_calls: vec![HelpMinimalCall {
                tool: "audit".into(),
                arguments: json!({
                    "agent_id": "jimi",
                    "path": path.unwrap_or_else(|| "/path/to/project".into()),
                    "profile": "auto"
                }),
            }],
            rejected_alternatives: vec![HelpRejectedAlternative {
                tool: "search".into(),
                reason: "Do not start with text search when you do not yet know the repo shape or likely surface.".into(),
            }],
            recovery_steps: vec!["If the repo is still too broad after audit, use batch_view or activate on the strongest finding.".into()],
            next_step: Some("Run audit first, then inspect the strongest finding.".into()),
            why: "Orientation work should start with a top-level structural pass, not a blind string hunt.".into(),
            confidence: 0.92,
            canonical_name: None,
            aliases: vec![],
        },
        HelpStage::Ground => {
            let adapter = if intent.to_lowercase().contains("light") {
                "light"
            } else {
                "universal"
            };
            HelpGuidance {
                decision_type: "sequence".into(),
                recommended_tools: vec!["ingest".into(), "document_bindings".into()],
                recommended_sequence: vec![
                    HelpSequenceStep {
                        tool: "ingest".into(),
                        reason: format!("Ground the document or spec into the graph with adapter=\"{}\".", adapter),
                    },
                    HelpSequenceStep {
                        tool: "document_bindings".into(),
                        reason: "Resolve the strongest implementation bindings after ingest.".into(),
                    },
                ],
                required_inputs: vec!["path".into()],
                missing_context: input
                    .path
                    .as_ref()
                    .map(|_| Vec::new())
                    .unwrap_or_else(|| vec!["path".into()]),
                minimal_calls: vec![HelpMinimalCall {
                    tool: "ingest".into(),
                    arguments: json!({
                        "agent_id": "jimi",
                        "path": input.path.clone().unwrap_or_else(|| "docs/spec.md".into()),
                        "adapter": adapter,
                        "mode": "merge"
                    }),
                }],
                rejected_alternatives: vec![HelpRejectedAlternative {
                    tool: "search".into(),
                    reason: "Do not search a spec or PDF blind when you can ingest and bind it to code first.".into(),
                }],
                recovery_steps: vec!["If the document is already ingested, move straight to document_bindings or document_drift.".into()],
                next_step: Some("Ingest the document, then bind it back to code.".into()),
                why: "Docs/spec work is strongest after the source is grounded into the graph.".into(),
                confidence: 0.9,
                canonical_name: None,
                aliases: vec![],
            }
        }
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
                    tool: "apply_batch".into(),
                    reason: "Do not execute before impact and plan validity are strong enough.".into(),
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
                    recommended_tools: vec!["apply_batch".into(), "predict".into()],
                    recommended_sequence: vec![
                        HelpSequenceStep {
                            tool: "apply_batch".into(),
                            reason: "The flow already passed through proof; execute atomically and verify.".into(),
                        },
                        HelpSequenceStep {
                            tool: "predict".into(),
                            reason: "Check ripple effects after the write surface completes.".into(),
                        },
                    ],
                    required_inputs: vec!["edits".into()],
                    missing_context: vec!["edits".into()],
                    minimal_calls: vec![HelpMinimalCall {
                        tool: "apply_batch".into(),
                        arguments: json!({
                            "agent_id": "jimi",
                            "edits": [{
                                "file_path": "src/example.rs",
                                "new_content": "...",
                                "description": "Apply the validated change"
                            }],
                            "atomic": true,
                            "reingest": true,
                            "verify": true
                        }),
                    }],
                    rejected_alternatives: vec![HelpRejectedAlternative {
                        tool: "search".into(),
                        reason: "The search phase should already be over if you are at execution.".into(),
                    }],
                    recovery_steps: vec!["If execution still feels premature, step back to surgical_context_v2 or validate_plan instead of forcing the write.".into()],
                    next_step: Some("Run apply_batch with the validated edit set.".into()),
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
                        "apply_batch".into(),
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
                            tool: "apply_batch".into(),
                            reason: "Execute only after the plan is no longer proving.".into(),
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
                        tool: "apply_batch".into(),
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
            recovery_steps: vec!["If you do not yet know the changed seam, start from audit(path) instead.".into()],
            next_step: Some("Run timeline on the changed seam, then inspect impact.".into()),
            why: "Review mode should pull in history and blast radius before opening a broad file set.".into(),
            confidence: 0.8,
            canonical_name: None,
            aliases: vec![],
        },
        HelpStage::Operate => HelpGuidance {
            decision_type: "sequence".into(),
            recommended_tools: vec!["audit".into(), "daemon_status".into(), "alerts_list".into()],
            recommended_sequence: vec![
                HelpSequenceStep {
                    tool: "audit".into(),
                    reason: "Get a current operational health pass over topology, verification, and git state.".into(),
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
            required_inputs: vec!["path".into()],
            missing_context: input
                .path
                .as_ref()
                .map(|_| Vec::new())
                .unwrap_or_else(|| vec!["path".into()]),
            minimal_calls: vec![HelpMinimalCall {
                tool: "audit".into(),
                arguments: json!({
                    "agent_id": "jimi",
                    "path": input.path.clone().unwrap_or_else(|| "/path/to/project".into()),
                    "profile": "production"
                }),
            }],
            rejected_alternatives: vec![HelpRejectedAlternative {
                tool: "search".into(),
                reason: "Operations work needs system state, not just text retrieval.".into(),
            }],
            recovery_steps: vec!["If you need continuous monitoring, move from audit to daemon_start rather than polling search.".into()],
            next_step: Some("Run audit first, then inspect daemon status and alerts.".into()),
            why: "Operations mode should start from repo or runtime health signals rather than content search.".into(),
            confidence: 0.84,
            canonical_name: None,
            aliases: vec![],
        },
        HelpStage::Handoff => {
            let resume = input.intent.as_deref().unwrap_or_default().to_lowercase().contains("resume");
            if resume {
                HelpGuidance {
                    decision_type: "sequence".into(),
                    recommended_tools: vec!["trail_resume".into(), "coverage_session".into()],
                    recommended_sequence: vec![
                        HelpSequenceStep {
                            tool: "trail_resume".into(),
                            reason: "Rehydrate the saved investigation and recover the next likely step.".into(),
                        },
                        HelpSequenceStep {
                            tool: "coverage_session".into(),
                            reason: "Check what has and has not been inspected in this session.".into(),
                        },
                    ],
                    required_inputs: vec!["trail_id".into()],
                    missing_context: vec!["trail_id".into()],
                    minimal_calls: vec![HelpMinimalCall {
                        tool: "trail_resume".into(),
                        arguments: json!({
                            "agent_id": "jimi",
                            "trail_id": "trail_auth_flow"
                        }),
                    }],
                    rejected_alternatives: vec![HelpRejectedAlternative {
                        tool: "search".into(),
                        reason: "Do not restart a saved investigation as a blind fresh search.".into(),
                    }],
                    recovery_steps: vec!["If there is no saved trail yet, use trail_save before ending the current session.".into()],
                    next_step: Some("Resume the saved trail, then inspect the suggested next move.".into()),
                    why: "Handoff or resume mode should preserve and reactivate investigation state instead of rebuilding it.".into(),
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
                    recovery_steps: vec!["If the work must stay hot across restarts, complement the trail with boot_memory for short canonical doctrine.".into()],
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
            recommended_tools: vec!["audit".into(), "glob".into()],
            recommended_sequence: vec![
                HelpSequenceStep {
                    tool: "audit".into(),
                    reason: "Normalize the workspace root or repo path before narrower operations.".into(),
                },
                HelpSequenceStep {
                    tool: "glob".into(),
                    reason: "Confirm the path pattern once the workspace root is right.".into(),
                },
            ],
            required_inputs: vec!["path".into()],
            missing_context: vec!["A canonical path or scope".into()],
            minimal_calls: vec![HelpMinimalCall {
                tool: "audit".into(),
                arguments: json!({
                    "agent_id": "jimi",
                    "path": input.path.clone().unwrap_or_else(|| "/path/to/project".into())
                }),
            }],
            rejected_alternatives: vec![HelpRejectedAlternative {
                tool: "search".into(),
                reason: "Do not keep searching until the path or scope is canonicalized.".into(),
            }],
            recovery_steps: vec!["Fix the path/scope first, then re-run the narrower tool.".into()],
            next_step: Some("Canonicalize the path with audit, then narrow again.".into()),
            why: "Path and scope failures usually mean the tool choice is fine but the target is not canonical yet.".into(),
            confidence: 0.83,
            canonical_name: None,
            aliases: vec![],
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
    let repo_path = input
        .path
        .clone()
        .unwrap_or_else(|| "/path/to/project".into());
    vec![
        OverviewCard {
            title: "Need repo orientation",
            why: "Start from a one-call structural pass before narrower retrieval.",
            tool: "audit",
            arguments: json!({
                "agent_id": "jimi",
                "path": repo_path,
                "profile": "auto"
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
            why: "Ground the document first, then bind it back to code.",
            tool: "ingest",
            arguments: json!({
                "agent_id": "jimi",
                "path": "docs/spec.md",
                "adapter": "universal",
                "mode": "merge"
            }),
            reject_tool: "search",
            reject_reason: "Searching an ungrounded document loses bindings and drift detection.",
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
            reject_tool: "apply_batch",
            reject_reason: "Batch execution is too early until the change stops proving and becomes safe to execute.",
        },
        OverviewCard {
            title: "Need continuity or handoff",
            why: "Persist or resume investigation state explicitly.",
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
    let width = 72;
    let mut out = String::new();

    out.push_str(&gradient_top_border(width));
    out.push('\n');
    out.push_str(&format!(
        "{}{} {}  {}{}{}\n",
        ANSI_CYAN,
        entry.glyph,
        entry.category.to_uppercase(),
        ANSI_BOLD,
        entry.name,
        ANSI_RESET
    ));
    out.push_str(&format!(
        "{}canonical: {}{}",
        ANSI_DIM, entry.name, ANSI_RESET
    ));
    if !entry.aliases.is_empty() {
        out.push_str(&format!(
            "{}  aliases: {}{}{}\n",
            ANSI_DIM,
            ANSI_BOLD,
            entry.aliases.join(", "),
            ANSI_RESET
        ));
    } else {
        out.push('\n');
    }
    out.push('\n');

    render_section_title(&mut out, "USE THIS NOW");
    out.push_str(&format!(
        "  {}{}{}{}\n\n",
        ANSI_CYAN, ANSI_BOLD, entry.name, ANSI_RESET
    ));

    render_section_title(&mut out, "WHY");
    out.push_str(&format!(
        "  {}- {}{}\n",
        ANSI_DIM, entry.one_liner, ANSI_RESET
    ));
    if !entry.returns.is_empty() {
        out.push_str(&format!(
            "  {}- Returns: {}{}\n",
            ANSI_DIM, entry.returns, ANSI_RESET
        ));
    }
    for line in personality::when_to_use(&entry.name)
        .iter()
        .take(if compact { 1 } else { 2 })
    {
        out.push_str(&format!("  {}- {}{}\n", ANSI_DIM, line, ANSI_RESET));
    }
    out.push('\n');

    render_section_title(&mut out, "REQUIRED INPUT");
    let required: Vec<&HelpParamSpec> =
        entry.params.iter().filter(|param| param.required).collect();
    if required.is_empty() {
        out.push_str(&format!(
            "  {}- no required inputs beyond the call itself{}\n",
            ANSI_DIM, ANSI_RESET
        ));
    } else {
        for param in required {
            out.push_str(&format!(
                "  {}- {}{}{} — {}{}\n",
                ANSI_DIM, ANSI_BOLD, param.name, ANSI_RESET, param.description, ANSI_RESET
            ));
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
            "  {}- optional knobs: {}{}\n",
            ANSI_DIM,
            optional
                .into_iter()
                .take(max)
                .collect::<Vec<_>>()
                .join(", "),
            ANSI_RESET
        ));
    }
    out.push('\n');

    render_section_title(&mut out, "MINIMAL CALL");
    if let Some(call) = guidance.minimal_calls.first() {
        out.push_str(&format!(
            "  {}{}{}({})\n\n",
            ANSI_MAGENTA,
            call.tool,
            ANSI_RESET,
            render_inline_json(&call.arguments)
        ));
    }

    render_section_title(&mut out, "DO NOT USE IF");
    let avoid = personality::avoid_when(&entry.name);
    if avoid.is_empty() && guidance.rejected_alternatives.is_empty() {
        out.push_str(&format!(
            "  {}- the retrieval or reasoning shape you need is fundamentally different{}\n\n",
            ANSI_DIM, ANSI_RESET
        ));
    } else {
        for line in avoid.iter().take(if compact { 1 } else { 2 }) {
            out.push_str(&format!("  {}- {}{}\n", ANSI_DIM, line, ANSI_RESET));
        }
        for alternative in guidance
            .rejected_alternatives
            .iter()
            .take(if compact { 1 } else { 2 })
        {
            out.push_str(&format!(
                "  {}- do not substitute with `{}`: {}{}\n",
                ANSI_DIM, alternative.tool, alternative.reason, ANSI_RESET
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
        out.push_str(&format!("  {}- {}{}\n", ANSI_DIM, step, ANSI_RESET));
    }
    out.push('\n');

    render_section_title(&mut out, "NEXT");
    let next = if entry.next.is_empty() {
        vec!["help".into()]
    } else {
        entry.next.clone()
    };
    for tool in next.iter().take(if compact { 2 } else { 4 }) {
        out.push_str(&format!(
            "  {}- {}{}{}{}\n",
            ANSI_CYAN, ANSI_BOLD, tool, ANSI_RESET, ANSI_RESET
        ));
    }
    out.push('\n');
    out.push_str(&gradient_bottom_border(width));
    out.push('\n');
    out
}

fn render_overview(
    total_tools: usize,
    cards: &[OverviewCard],
    compact: bool,
    show_temponizer: bool,
) -> String {
    let width = 72;
    let mut out = String::new();
    out.push_str(&gradient_top_border(width));
    out.push('\n');
    out.push_str(&format!(
        "{}{}  m1nd help{}\n",
        ANSI_BOLD, ANSI_CYAN, ANSI_RESET
    ));
    out.push_str(&format!(
        "{}  {} live tools. route by stage, intent, or failure.{}\n",
        ANSI_DIM, total_tools, ANSI_RESET
    ));
    out.push_str(&format!(
        "{}  structure · change · docs · operations · continuity{}\n\n",
        ANSI_DIM, ANSI_RESET
    ));

    render_section_title(&mut out, "30-SECOND MAP");
    for card in cards.iter().take(if compact { 4 } else { cards.len() }) {
        out.push_str(&format!(
            "  {}- {}{}{} -> {}{}{} — {}{}\n",
            ANSI_DIM,
            ANSI_BOLD,
            card.title,
            ANSI_RESET,
            ANSI_CYAN,
            card.tool,
            ANSI_RESET,
            card.why,
            ANSI_RESET
        ));
    }
    out.push('\n');

    render_section_title(&mut out, "HOW TO ASK");
    out.push_str(&format!(
        "  {}- help(tool_name=\"activate\") for exact tool doctrine{}\n",
        ANSI_DIM, ANSI_RESET
    ));
    out.push_str(&format!(
        "  {}- help(stage=\"plan\", intent=\"validate a risky change\") for a route{}\n",
        ANSI_DIM, ANSI_RESET
    ));
    out.push_str(&format!(
        "  {}- help(error_text=\"...\") for failure recovery{}\n",
        ANSI_DIM, ANSI_RESET
    ));
    out.push_str(&format!(
        "  {}- help(mode=\"workflow\", stage=\"edit\") for a full sequence{}\n",
        ANSI_DIM, ANSI_RESET
    ));

    if show_temponizer {
        out.push('\n');
        render_section_title(&mut out, "TEMPONIZER");
        out.push_str(&format!(
            "  {}- you estimate in human-time but execute much faster; fill the recovered time with more deliverables, not padding.{}\n",
            ANSI_DIM, ANSI_RESET
        ));
    }

    out.push('\n');
    out.push_str(&gradient_bottom_border(width));
    out.push('\n');
    out
}

fn render_guidance(title: &str, guidance: &HelpGuidance, compact: bool) -> String {
    let width = 72;
    let mut out = String::new();
    out.push_str(&gradient_top_border(width));
    out.push('\n');
    out.push_str(&format!(
        "{}{}  help {}{}\n\n",
        ANSI_BOLD, ANSI_CYAN, title, ANSI_RESET
    ));

    render_section_title(&mut out, "USE THIS NOW");
    for tool in guidance
        .recommended_tools
        .iter()
        .take(if compact { 2 } else { 4 })
    {
        out.push_str(&format!(
            "  {}- {}{}{}{}\n",
            ANSI_CYAN, ANSI_BOLD, tool, ANSI_RESET, ANSI_RESET
        ));
    }
    out.push('\n');

    render_section_title(&mut out, "WHY");
    out.push_str(&format!(
        "  {}- {}{}\n\n",
        ANSI_DIM, guidance.why, ANSI_RESET
    ));

    render_section_title(&mut out, "REQUIRED INPUT");
    if guidance.required_inputs.is_empty() {
        out.push_str(&format!(
            "  {}- no extra input beyond the current state{}\n",
            ANSI_DIM, ANSI_RESET
        ));
    } else {
        out.push_str(&format!(
            "  {}- {}{}\n",
            ANSI_DIM,
            guidance.required_inputs.join(", "),
            ANSI_RESET
        ));
    }
    if !guidance.missing_context.is_empty() {
        out.push_str(&format!(
            "  {}- still missing: {}{}\n",
            ANSI_DIM,
            guidance.missing_context.join(", "),
            ANSI_RESET
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
            "  {}{}{}({})\n",
            ANSI_MAGENTA,
            call.tool,
            ANSI_RESET,
            render_inline_json(&call.arguments)
        ));
    }
    out.push('\n');

    render_section_title(&mut out, "DO NOT USE IF");
    if guidance.rejected_alternatives.is_empty() {
        out.push_str(&format!(
            "  {}- the uncertainty shape has changed and now needs a different first tool{}\n",
            ANSI_DIM, ANSI_RESET
        ));
    } else {
        for alternative in guidance
            .rejected_alternatives
            .iter()
            .take(if compact { 1 } else { 3 })
        {
            out.push_str(&format!(
                "  {}- `{}`: {}{}\n",
                ANSI_DIM, alternative.tool, alternative.reason, ANSI_RESET
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
        out.push_str(&format!("  {}- {}{}\n", ANSI_DIM, step, ANSI_RESET));
    }
    out.push('\n');

    render_section_title(&mut out, "NEXT");
    for step in guidance
        .recommended_sequence
        .iter()
        .take(if compact { 2 } else { 4 })
    {
        out.push_str(&format!(
            "  {}- {}{}{} — {}{}\n",
            ANSI_CYAN, ANSI_BOLD, step.tool, ANSI_RESET, step.reason, ANSI_RESET
        ));
    }
    if let Some(next_step) = &guidance.next_step {
        out.push_str(&format!("  {}- {}{}\n", ANSI_DIM, next_step, ANSI_RESET));
    }
    out.push('\n');
    out.push_str(&gradient_bottom_border(width));
    out.push('\n');
    out
}

fn render_unknown_tool(query: &str, suggestions: &[String]) -> String {
    let width = 72;
    let mut out = String::new();
    out.push_str(&gradient_top_border(width));
    out.push('\n');
    out.push_str(&format!(
        "{}{}  help recovery{}\n\n",
        ANSI_BOLD, ANSI_RED, ANSI_RESET
    ));
    out.push_str(&format!(
        "{}unknown tool:{} {}{}{}\n",
        ANSI_RED, ANSI_RESET, ANSI_BOLD, query, ANSI_RESET
    ));
    if suggestions.is_empty() {
        out.push_str(&format!(
            "{}no similar live tool names found. Use stage or intent instead of guessing.{}\n",
            ANSI_DIM, ANSI_RESET
        ));
    } else {
        out.push_str(&format!(
            "{}did you mean:{} {}{}{}\n",
            ANSI_DIM,
            ANSI_RESET,
            ANSI_BOLD,
            suggestions.join(", "),
            ANSI_RESET
        ));
    }
    out.push('\n');
    out.push_str(&gradient_bottom_border(width));
    out.push('\n');
    out
}

fn render_section_title(out: &mut String, title: &str) {
    out.push_str(&format!("{}{}{}\n", ANSI_GOLD, title, ANSI_RESET));
}

fn render_inline_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".into())
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
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
}
