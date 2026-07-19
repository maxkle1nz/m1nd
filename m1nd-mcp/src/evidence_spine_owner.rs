//! Owner-side adapters for the G5 evidence projection.
//!
//! These adapters never accept a raw receipt, mission letter, or evidence event
//! from a client. G3 installs the spine identity and canonical core events. The
//! delegation and Mission Control adapters may only consume an owner-emitted
//! [`EvidenceCorrelationLinkV1`] that still resolves to an existing G3 anchor.

use std::path::{Path, PathBuf};

use m1nd_core::error::{M1ndError, M1ndResult};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::evidence_spine::{
    EvidenceAppendDisposition, EvidenceCausalAttachmentV1, EvidenceCorrelationLinkV1,
    EvidenceMissionBindingV1, EvidenceSpineError, EvidenceSpineQueryV1, EvidenceSpineStore,
};
use crate::session::SessionState;

pub const OWNER_EVIDENCE_SPINE_DIR: &str = "evidence-spine";
pub const EVIDENCE_PROJECTION_STATUS_SCHEMA: &str = "m1nd-evidence-projection-status-v1";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceQueryInputV1 {
    #[serde(default)]
    correlation_id: Option<String>,
    #[serde(default)]
    mission_id: Option<String>,
    #[serde(default)]
    mission_head_id: Option<String>,
    #[serde(default)]
    transaction_id: Option<String>,
    #[serde(default)]
    receipt_id: Option<String>,
    #[serde(default)]
    delegation_id: Option<String>,
    #[serde(default)]
    mission_control_id: Option<String>,
}

pub fn root_for_state(state: &SessionState) -> PathBuf {
    state.runtime_root.join(OWNER_EVIDENCE_SPINE_DIR)
}

pub fn parse_optional_link(
    tool: &'static str,
    params: &Value,
) -> M1ndResult<Option<EvidenceCorrelationLinkV1>> {
    let Some(value) = params.get("evidence_link") else {
        return Ok(None);
    };
    serde_json::from_value(value.clone())
        .map(Some)
        .map_err(|error| invalid(tool, format!("invalid evidence_link: {error}")))
}

/// Validate an explicit link before a coordination authority commits its own
/// record. The client supplies only correlation coordinates; organism, brain,
/// and workspace come from the owner-installed spine identity.
pub fn validate_link(
    state: &SessionState,
    tool: &'static str,
    link: &EvidenceCorrelationLinkV1,
) -> M1ndResult<()> {
    let workspace = selected_workspace(state, tool)?;
    let store = EvidenceSpineStore::open_existing_for_workspace(root_for_state(state), &workspace)
        .map_err(|error| map_error(tool, error))?;
    let binding = link
        .binding(store.identity())
        .map_err(|error| map_error(tool, error))?;
    let attachment = link.attachment().map_err(|error| map_error(tool, error))?;
    store
        .validate_authority_anchor(&binding, &attachment)
        .map_err(|error| map_error(tool, error))
}

pub fn validate_record_workspace(
    state: &SessionState,
    tool: &'static str,
    record_workspace: &str,
) -> M1ndResult<()> {
    let selected = selected_workspace(state, tool)?;
    let selected = std::fs::canonicalize(&selected).map_err(|error| {
        invalid(
            tool,
            format!("selected workspace '{selected}' cannot be canonicalized: {error}"),
        )
    })?;
    let observed = std::fs::canonicalize(record_workspace).map_err(|error| {
        invalid(
            tool,
            format!("record workspace '{record_workspace}' cannot be canonicalized: {error}"),
        )
    })?;
    if observed != selected {
        return Err(invalid(
            tool,
            "wrong_workspace_binding: coordination record workspace differs from the selected owner brain",
        ));
    }
    Ok(())
}

pub fn record_delegation_packet(
    state: &SessionState,
    link: Option<&EvidenceCorrelationLinkV1>,
    packet: &Value,
    observed_at: u64,
) -> Value {
    project_optional_link(state, link, "delegate", |store, binding, attachment| {
        store.record_delegation_packet(binding, attachment, packet, observed_at)
    })
}

pub fn record_delegation_outcome(
    state: &SessionState,
    link: Option<&EvidenceCorrelationLinkV1>,
    outcome_record: &Value,
    observed_at: u64,
) -> Value {
    project_optional_link(state, link, "debrief", |store, binding, attachment| {
        store.record_delegation_outcome(binding, attachment, outcome_record, observed_at)
    })
}

pub fn record_mission_control(
    state: &SessionState,
    link: Option<&EvidenceCorrelationLinkV1>,
    record: &Value,
    observed_at: u64,
) -> Value {
    project_optional_link(
        state,
        link,
        "mission_control",
        |store, binding, attachment| {
            store.record_mission_control(binding, attachment, record, observed_at)
        },
    )
}

/// Real read-only EvidenceQuery. It reads and verifies the committed JSONL
/// prefix without creating locks, repairing tails, or writing cache state.
pub fn handle_evidence_query(state: &SessionState, params: &Value) -> M1ndResult<Value> {
    let input: EvidenceQueryInputV1 = serde_json::from_value(params.clone()).map_err(|error| {
        invalid(
            "evidence_query",
            format!("invalid evidence query input: {error}"),
        )
    })?;
    let workspace = selected_workspace(state, "evidence_query")?;
    let query = EvidenceSpineQueryV1 {
        correlation_id: input.correlation_id,
        mission_id: input.mission_id,
        mission_head_id: input.mission_head_id,
        transaction_id: input.transaction_id,
        receipt_id: input.receipt_id,
        delegation_id: input.delegation_id,
        mission_control_id: input.mission_control_id,
    };
    let result = EvidenceSpineStore::query_existing_read_only(
        root_for_state(state),
        Path::new(&workspace),
        &query,
    )
    .map_err(|error| map_error("evidence_query", error))?;
    serde_json::to_value(result).map_err(M1ndError::Serde)
}

pub fn gap_status(code: &str, detail: impl Into<String>) -> Value {
    json!({
        "schema": EVIDENCE_PROJECTION_STATUS_SCHEMA,
        "status": "gap",
        "code": code,
        "detail": detail.into(),
        "non_claim": "the coordination record remains canonical in its own store; no cross-surface correlation was fabricated",
    })
}

fn project_optional_link<F>(
    state: &SessionState,
    link: Option<&EvidenceCorrelationLinkV1>,
    source: &'static str,
    record: F,
) -> Value
where
    F: FnOnce(
        &mut EvidenceSpineStore,
        &EvidenceMissionBindingV1,
        EvidenceCausalAttachmentV1,
    ) -> Result<crate::evidence_spine::EvidenceAppendOutcomeV1, EvidenceSpineError>,
{
    let Some(link) = link else {
        return gap_status(
            "canonical_evidence_link_absent",
            format!(
                "{source} has no owner-emitted evidence_link; it cannot be joined to a G3 mission"
            ),
        );
    };
    let result = (|| {
        let workspace = selected_workspace(state, source)?;
        let mut store = EvidenceSpineStore::open_existing_for_workspace(
            root_for_state(state),
            Path::new(&workspace),
        )
        .map_err(|error| map_error(source, error))?;
        let binding = link
            .binding(store.identity())
            .map_err(|error| map_error(source, error))?;
        let attachment = link
            .attachment()
            .map_err(|error| map_error(source, error))?;
        store
            .validate_authority_anchor(&binding, &attachment)
            .map_err(|error| map_error(source, error))?;
        record(&mut store, &binding, attachment).map_err(|error| map_error(source, error))
    })();
    match result {
        Ok(outcome) => json!({
            "schema": EVIDENCE_PROJECTION_STATUS_SCHEMA,
            "status": match outcome.disposition {
                EvidenceAppendDisposition::Appended => "appended",
                EvidenceAppendDisposition::Replayed => "replayed",
            },
            "event_id": outcome.event_id,
            "correlation_id": outcome.correlation_id,
            "sequence": outcome.sequence,
            "evidence_link": link,
        }),
        Err(error) => gap_status("evidence_projection_failed", error.to_string()),
    }
}

fn selected_workspace(state: &SessionState, tool: &'static str) -> M1ndResult<String> {
    crate::delegation_handlers::binding_workspace_root(state).ok_or_else(|| {
        invalid(
            tool,
            "selected brain has no canonical project/workspace root",
        )
    })
}

fn map_error(tool: &'static str, error: EvidenceSpineError) -> M1ndError {
    invalid(tool, format!("{}: {error}", error.code()))
}

fn invalid(tool: &'static str, detail: impl Into<String>) -> M1ndError {
    M1ndError::InvalidParams {
        tool: tool.to_string(),
        detail: detail.into(),
    }
}
