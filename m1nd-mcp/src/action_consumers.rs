//! Exhaustive owner-side consumer coverage for the M1ND-10 action catalog.
//!
//! The action catalog says what an action means and which ingresses can reach
//! it. This registry says whether each `(action, ingress)` cell has an exact
//! consumer. It is deliberately a full cartesian matrix: absence is recorded
//! as policy, never inferred from a missing row.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use m1nd_control::{
    digest_canonical, m1nd10_action_catalog, ActionId, AuthorityFloor, Effect, Ingress, RiskClass,
    M1ND10_ACTION_CATALOG_VERSION,
};
use serde::{Deserialize, Serialize};

pub const ACTION_CONSUMER_REGISTRY_SCHEMA: &str = "m1nd-action-consumer-registry-v1";
pub const ACTION_CONSUMER_REGISTRY_DIGEST_DOMAIN: &str = "m1nd-action-consumer-registry-v1";
pub const ACTION_CONSUMER_REGISTRY_VERSION: &str = "m1nd10-g2-consumers-2026-07-19.1";
pub const ACTION_CONSUMER_CONTRACT_SCHEMA: &str = "m1nd-action-consumer-contract-v1";

/// Source-revision pin. A catalog edit must deliberately update the consumer
/// declarations and this digest together; silently regenerating the registry
/// from a changed catalog is forbidden.
pub const EXPECTED_M1ND10_ACTION_CATALOG_DIGEST: &str =
    "fc1ca39db27b1cd03b8e69cfe260ebf6c77753609626f484ac39dd33f7a1def7";
pub const EXPECTED_M1ND10_ACTION_COUNT: usize = 174;

pub const MISSION_SERVICE_CONTRACT_VERSION: &str = "m1nd-mission-service-transport-request-v1";
pub const EXTERNAL_MUTATION_SERVICE_CONTRACT_VERSION: &str = "m1nd-external-mutation-consumer-v1";

pub const ALL_ACTION_INGRESSES: [Ingress; 7] = [
    Ingress::Mcp,
    Ingress::Rest,
    Ingress::Cli,
    Ingress::Hook,
    Ingress::BackgroundJob,
    Ingress::Recovery,
    Ingress::Migration,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypedConsumerIdV1 {
    MissionService,
    ExternalMutationService,
}

/// Concrete wire accepted by a typed consumer. `Ingress::Mcp` is the action
/// catalog's semantic ingress; it does not by itself authorize stdio MCP. The
/// two typed services in this revision accept only Streamable-HTTP MCP (or the
/// explicitly declared REST facade for MissionService).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypedConsumerTransportV1 {
    Rest,
    McpStreamableHttp,
}

impl TypedConsumerIdV1 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissionService => "mission_service",
            Self::ExternalMutationService => "external_mutation_service",
        }
    }

    const fn accepts_floor(self, floor: AuthorityFloor) -> bool {
        match self {
            Self::MissionService => matches!(
                floor,
                AuthorityFloor::Ordinary
                    | AuthorityFloor::ScopedGrantA2
                    | AuthorityFloor::PositiveSovereign
                    | AuthorityFloor::ServiceIdentity
            ),
            Self::ExternalMutationService => matches!(
                floor,
                AuthorityFloor::Ordinary
                    | AuthorityFloor::ScopedGrantA2
                    | AuthorityFloor::PositiveSovereign
            ),
        }
    }

    const fn is_positive_mutation_consumer(self) -> bool {
        matches!(self, Self::ExternalMutationService)
    }
}

impl fmt::Display for TypedConsumerIdV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsumerPolicyDisabledReasonV1 {
    NotDeclared,
    NoExactConsumer,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "disposition", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionConsumerDispositionV1 {
    EnabledGenericOrdinary,
    /// Admitted on the generic ingress ABOVE the ordinary floor, with no lease
    /// plane, because the action is named one by one in
    /// [`GENERIC_A2_LOCAL_ADMITTED_ACTIONS`]. This exists so the registry keeps
    /// telling the truth: an action reachable through the generic door must not
    /// be recorded here as `PolicyDisabled`, or the one artifact whose whole job
    /// is "absence is recorded as policy, never inferred" would lie about
    /// presence instead.
    EnabledGenericScopedA2Local,
    EnabledTypedConsumer {
        consumer_id: TypedConsumerIdV1,
        contract_version: String,
        transport: TypedConsumerTransportV1,
    },
    PolicyDisabled {
        reason: ConsumerPolicyDisabledReasonV1,
    },
}

/// The action-keyed generic-dispatch allowlist — the ONE opening in the
/// authority wall (`docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §1.1, owner-ratified
/// 2026-07-29).
///
/// Keyed BY ACTION, never by floor. That is the whole point of the shape: the
/// two `SCOPED_GRANT_A2` siblings that already exist — `source.edit.commit` and
/// `graph.ingest.merge_existing` — sit at exactly this floor and must stay
/// refused, so a floor-keyed exception would have opened all three at once
/// (verdict RC-4). `internal_tests/spec1_refresh_declared_root.rs` §5.9 pins
/// their refusal bytes verbatim against that mistake.
///
/// Admission here only admits the CATEGORY. Every authority-relevant fact —
/// which root, whether the caller exactly inhabits it, whether the candidate
/// would shrink the graph — is enforced inside the typed handler, after brain
/// resolution, fail-closed.
///
/// ONE source of truth: `server::enforce_generic_action_policy` reads this list
/// to admit, and `expected_disposition` reads it to describe. They cannot drift.
pub const GENERIC_A2_LOCAL_ADMITTED_ACTIONS: &[&str] = &["graph.ingest.refresh_declared_root"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionConsumerCellV1 {
    pub action: ActionId,
    pub ingress: Ingress,
    pub declared_ingress: bool,
    pub expected_effects: BTreeSet<Effect>,
    pub risk_class: RiskClass,
    pub authority_floor: AuthorityFloor,
    pub disposition: ActionConsumerDispositionV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionConsumerRegistryV1 {
    pub schema: String,
    pub registry_version: String,
    pub action_catalog_version: String,
    pub action_catalog_digest: String,
    pub cells: Vec<ActionConsumerCellV1>,
    pub registry_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionConsumerRegistryValidationV1 {
    pub action_count: usize,
    pub ingress_count: usize,
    pub cell_count: usize,
    pub declared_cell_count: usize,
    pub typed_consumer_cell_count: usize,
    pub computed_registry_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionConsumerContractV1 {
    pub schema: String,
    pub action: ActionId,
    pub ingress: Ingress,
    pub expected_effects: BTreeSet<Effect>,
    pub risk_class: RiskClass,
    pub authority_floor: AuthorityFloor,
    pub consumer_id: TypedConsumerIdV1,
    pub contract_version: String,
    pub transport: TypedConsumerTransportV1,
    pub action_catalog_version: String,
    pub action_catalog_digest: String,
    pub consumer_registry_version: String,
    pub consumer_registry_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumerPolicyDisabledV1 {
    pub action: String,
    pub ingress: Ingress,
    pub reason: ConsumerPolicyDisabledReasonV1,
    pub detail: String,
    pub action_catalog_digest: Option<String>,
    pub consumer_registry_digest: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionConsumerRegistryErrorCodeV1 {
    CatalogInvalid,
    CatalogDrift,
    RegistryShape,
    RegistryDigestMismatch,
    DuplicateCell,
    MissingCell,
    CellCatalogMismatch,
    DispositionMismatch,
    DuplicateConsumerDeclaration,
    ConsumerActionMissing,
    ConsumerIngressMismatch,
    ConsumerFloorMismatch,
    ConsumerEffectsMismatch,
    ConsumerRiskMismatch,
    ConsumerAuthorityDomainMismatch,
    Canonicalization,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionConsumerRegistryError {
    pub code: ActionConsumerRegistryErrorCodeV1,
    pub detail: String,
}

impl ActionConsumerRegistryError {
    fn new(code: ActionConsumerRegistryErrorCodeV1, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for ActionConsumerRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.detail)
    }
}

impl Error for ActionConsumerRegistryError {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TypedConsumerDeclarationV1 {
    action: String,
    ingress: Ingress,
    expected_effects: BTreeSet<Effect>,
    expected_risk_class: RiskClass,
    expected_authority_floor: AuthorityFloor,
    consumer_id: TypedConsumerIdV1,
    contract_version: &'static str,
    transport: TypedConsumerTransportV1,
}

fn effects<const N: usize>(values: [Effect; N]) -> BTreeSet<Effect> {
    values.into_iter().collect()
}

fn declaration<const N: usize>(
    action: &str,
    ingress: Ingress,
    expected_effects: [Effect; N],
    expected_risk_class: RiskClass,
    expected_authority_floor: AuthorityFloor,
    consumer_id: TypedConsumerIdV1,
    contract_version: &'static str,
) -> TypedConsumerDeclarationV1 {
    TypedConsumerDeclarationV1 {
        action: action.to_string(),
        ingress,
        expected_effects: effects(expected_effects),
        expected_risk_class,
        expected_authority_floor,
        consumer_id,
        contract_version,
        transport: match ingress {
            Ingress::Rest => TypedConsumerTransportV1::Rest,
            Ingress::Mcp => TypedConsumerTransportV1::McpStreamableHttp,
            Ingress::Cli
            | Ingress::Hook
            | Ingress::BackgroundJob
            | Ingress::Recovery
            | Ingress::Migration => {
                unreachable!("typed declarations have no non-HTTP transport in this revision")
            }
        },
    }
}

fn typed_consumer_declarations() -> Vec<TypedConsumerDeclarationV1> {
    use AuthorityFloor::{Ordinary, PositiveSovereign, ScopedGrantA2, ServiceIdentity};
    use Effect::{
        CoordinationRecord, GraphMutation, HostFilesystemWrite, MissionStateWrite, Read,
        RuntimeStoreWrite, SourceFilesystemWrite, SovereignMutation,
    };
    use Ingress::{Mcp, Rest};
    use RiskClass::{Critical, High, Low, Medium};
    use TypedConsumerIdV1::{ExternalMutationService, MissionService};

    let mut declarations = Vec::new();
    for ingress in [Mcp, Rest] {
        declarations.extend([
            declaration(
                "mission.service.land_intent",
                ingress,
                [Read],
                Low,
                Ordinary,
                MissionService,
                MISSION_SERVICE_CONTRACT_VERSION,
            ),
            declaration(
                "mission.service.mission_transition",
                ingress,
                [MissionStateWrite, RuntimeStoreWrite, CoordinationRecord],
                Medium,
                Ordinary,
                MissionService,
                MISSION_SERVICE_CONTRACT_VERSION,
            ),
            declaration(
                "mission.service.execution_dispatch",
                ingress,
                [MissionStateWrite, RuntimeStoreWrite, CoordinationRecord],
                High,
                ScopedGrantA2,
                MissionService,
                MISSION_SERVICE_CONTRACT_VERSION,
            ),
            declaration(
                "mission.service.execution_started",
                ingress,
                [MissionStateWrite, RuntimeStoreWrite, CoordinationRecord],
                High,
                ServiceIdentity,
                MissionService,
                MISSION_SERVICE_CONTRACT_VERSION,
            ),
            declaration(
                "mission.service.execution_terminal",
                ingress,
                [MissionStateWrite, RuntimeStoreWrite, CoordinationRecord],
                High,
                ServiceIdentity,
                MissionService,
                MISSION_SERVICE_CONTRACT_VERSION,
            ),
            declaration(
                "mission.service.land",
                ingress,
                [
                    MissionStateWrite,
                    RuntimeStoreWrite,
                    CoordinationRecord,
                    SovereignMutation,
                ],
                Critical,
                PositiveSovereign,
                MissionService,
                MISSION_SERVICE_CONTRACT_VERSION,
            ),
        ]);
    }

    declarations.extend([
        declaration(
            "system_blocks.ratify",
            Mcp,
            [RuntimeStoreWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
            ExternalMutationService,
            EXTERNAL_MUTATION_SERVICE_CONTRACT_VERSION,
        ),
        declaration(
            "brain.promote",
            Mcp,
            [
                GraphMutation,
                RuntimeStoreWrite,
                HostFilesystemWrite,
                SovereignMutation,
            ],
            Critical,
            PositiveSovereign,
            ExternalMutationService,
            EXTERNAL_MUTATION_SERVICE_CONTRACT_VERSION,
        ),
        declaration(
            "source.edit.commit",
            Mcp,
            [SourceFilesystemWrite, RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
            ExternalMutationService,
            EXTERNAL_MUTATION_SERVICE_CONTRACT_VERSION,
        ),
        declaration(
            "graph.ingest.preview",
            Mcp,
            [Read],
            Low,
            Ordinary,
            ExternalMutationService,
            EXTERNAL_MUTATION_SERVICE_CONTRACT_VERSION,
        ),
        declaration(
            "graph.ingest.replace",
            Mcp,
            [GraphMutation, RuntimeStoreWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
            ExternalMutationService,
            EXTERNAL_MUTATION_SERVICE_CONTRACT_VERSION,
        ),
        declaration(
            "graph.ingest.merge_existing",
            Mcp,
            [GraphMutation, RuntimeStoreWrite],
            High,
            ScopedGrantA2,
            ExternalMutationService,
            EXTERNAL_MUTATION_SERVICE_CONTRACT_VERSION,
        ),
    ]);
    declarations
}

fn catalog_error(detail: impl Into<String>) -> ActionConsumerRegistryError {
    ActionConsumerRegistryError::new(ActionConsumerRegistryErrorCodeV1::CatalogInvalid, detail)
}

fn validate_typed_consumer_declarations(
    catalog: &m1nd_control::ActionCatalogV1,
    declarations: &[TypedConsumerDeclarationV1],
) -> Result<(), ActionConsumerRegistryError> {
    let entries: BTreeMap<&str, &m1nd_control::ActionCatalogEntryV1> = catalog
        .entries
        .iter()
        .map(|entry| (entry.action.as_str(), entry))
        .collect();
    let mut seen = BTreeSet::new();

    for declaration in declarations {
        let key = (declaration.action.as_str(), declaration.ingress);
        if !seen.insert(key) {
            return Err(ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::DuplicateConsumerDeclaration,
                format!(
                    "duplicate typed consumer declaration for {} on {:?}",
                    declaration.action, declaration.ingress
                ),
            ));
        }
        let entry = entries.get(declaration.action.as_str()).ok_or_else(|| {
            ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::ConsumerActionMissing,
                format!(
                    "typed consumer action '{}' is absent from the catalog",
                    declaration.action
                ),
            )
        })?;
        if !entry.ingresses.contains(&declaration.ingress) {
            return Err(ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::ConsumerIngressMismatch,
                format!(
                    "typed consumer {} on {:?} is not a catalog-declared ingress",
                    declaration.action, declaration.ingress
                ),
            ));
        }
        if entry.authority_floor == AuthorityFloor::SafetyOnly
            && declaration.consumer_id.is_positive_mutation_consumer()
        {
            return Err(ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::ConsumerAuthorityDomainMismatch,
                format!(
                    "negative-only SAFETY_ONLY action {} cannot use positive consumer {}",
                    declaration.action, declaration.consumer_id
                ),
            ));
        }
        if !declaration.consumer_id.accepts_floor(entry.authority_floor) {
            return Err(ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::ConsumerAuthorityDomainMismatch,
                format!(
                    "consumer {} cannot accept {:?} for {}",
                    declaration.consumer_id, entry.authority_floor, declaration.action
                ),
            ));
        }
        let transport_matches_ingress = matches!(
            (declaration.ingress, declaration.transport),
            (Ingress::Rest, TypedConsumerTransportV1::Rest)
                | (Ingress::Mcp, TypedConsumerTransportV1::McpStreamableHttp)
        );
        if !transport_matches_ingress {
            return Err(ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::ConsumerIngressMismatch,
                format!(
                    "typed consumer {} transport {:?} cannot serve catalog ingress {:?}",
                    declaration.action, declaration.transport, declaration.ingress
                ),
            ));
        }
        if entry.authority_floor != declaration.expected_authority_floor {
            return Err(ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::ConsumerFloorMismatch,
                format!(
                    "typed consumer {} expected {:?}, catalog has {:?}",
                    declaration.action, declaration.expected_authority_floor, entry.authority_floor
                ),
            ));
        }
        if entry.complete_effects != declaration.expected_effects {
            return Err(ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::ConsumerEffectsMismatch,
                format!(
                    "typed consumer {} effect contract differs from the catalog",
                    declaration.action
                ),
            ));
        }
        if entry.risk_class != declaration.expected_risk_class {
            return Err(ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::ConsumerRiskMismatch,
                format!(
                    "typed consumer {} expected {:?} risk, catalog has {:?}",
                    declaration.action, declaration.expected_risk_class, entry.risk_class
                ),
            ));
        }
        if declaration.contract_version.trim().is_empty() {
            return Err(ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::RegistryShape,
                format!(
                    "typed consumer {} has an empty contract version",
                    declaration.action
                ),
            ));
        }
    }
    Ok(())
}

fn declaration_map(
    catalog: &m1nd_control::ActionCatalogV1,
) -> Result<BTreeMap<(String, Ingress), TypedConsumerDeclarationV1>, ActionConsumerRegistryError> {
    let declarations = typed_consumer_declarations();
    validate_typed_consumer_declarations(catalog, &declarations)?;
    Ok(declarations
        .into_iter()
        .map(|declaration| {
            (
                (declaration.action.clone(), declaration.ingress),
                declaration,
            )
        })
        .collect())
}

fn expected_disposition(
    entry: &m1nd_control::ActionCatalogEntryV1,
    ingress: Ingress,
    declarations: &BTreeMap<(String, Ingress), TypedConsumerDeclarationV1>,
) -> ActionConsumerDispositionV1 {
    if let Some(declaration) = declarations.get(&(entry.action.to_string(), ingress)) {
        return ActionConsumerDispositionV1::EnabledTypedConsumer {
            consumer_id: declaration.consumer_id,
            contract_version: declaration.contract_version.to_string(),
            transport: declaration.transport,
        };
    }
    if !entry.ingresses.contains(&ingress) {
        return ActionConsumerDispositionV1::PolicyDisabled {
            reason: ConsumerPolicyDisabledReasonV1::NotDeclared,
        };
    }
    if entry.authority_floor == AuthorityFloor::Ordinary {
        return ActionConsumerDispositionV1::EnabledGenericOrdinary;
    }
    if GENERIC_A2_LOCAL_ADMITTED_ACTIONS.contains(&entry.action.as_str()) {
        return ActionConsumerDispositionV1::EnabledGenericScopedA2Local;
    }
    ActionConsumerDispositionV1::PolicyDisabled {
        reason: ConsumerPolicyDisabledReasonV1::NoExactConsumer,
    }
}

impl ActionConsumerRegistryV1 {
    pub fn compute_registry_digest(&self) -> Result<String, ActionConsumerRegistryError> {
        let mut value = serde_json::to_value(self).map_err(|error| {
            ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::Canonicalization,
                error.to_string(),
            )
        })?;
        let object = value.as_object_mut().ok_or_else(|| {
            ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::Canonicalization,
                "consumer registry did not serialize as an object",
            )
        })?;
        object.remove("registry_digest");
        digest_canonical(ACTION_CONSUMER_REGISTRY_DIGEST_DOMAIN, &value).map_err(|error| {
            ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::Canonicalization,
                error.to_string(),
            )
        })
    }

    fn seal(&mut self) -> Result<(), ActionConsumerRegistryError> {
        self.registry_digest = self.compute_registry_digest()?;
        Ok(())
    }

    pub fn cell(&self, action: &str, ingress: Ingress) -> Option<&ActionConsumerCellV1> {
        self.cells
            .iter()
            .find(|cell| cell.action.as_str() == action && cell.ingress == ingress)
    }

    pub fn validate(
        &self,
    ) -> Result<ActionConsumerRegistryValidationV1, ActionConsumerRegistryError> {
        if self.schema != ACTION_CONSUMER_REGISTRY_SCHEMA
            || self.registry_version != ACTION_CONSUMER_REGISTRY_VERSION
        {
            return Err(ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::RegistryShape,
                "consumer registry schema or version mismatch",
            ));
        }
        let catalog = m1nd10_action_catalog().map_err(|error| catalog_error(error.to_string()))?;
        catalog
            .validate()
            .map_err(|error| catalog_error(error.to_string()))?;
        if catalog.entries.len() != EXPECTED_M1ND10_ACTION_COUNT
            || catalog.catalog_version != M1ND10_ACTION_CATALOG_VERSION
            || catalog.catalog_digest != EXPECTED_M1ND10_ACTION_CATALOG_DIGEST
        {
            return Err(ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::CatalogDrift,
                format!(
                    "expected {} actions at {} digest {}, observed {} actions at {} digest {}",
                    EXPECTED_M1ND10_ACTION_COUNT,
                    M1ND10_ACTION_CATALOG_VERSION,
                    EXPECTED_M1ND10_ACTION_CATALOG_DIGEST,
                    catalog.entries.len(),
                    catalog.catalog_version,
                    catalog.catalog_digest
                ),
            ));
        }
        if self.action_catalog_version != catalog.catalog_version
            || self.action_catalog_digest != catalog.catalog_digest
        {
            return Err(ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::CatalogDrift,
                "consumer registry is not bound to the current pinned action catalog",
            ));
        }

        let computed_registry_digest = self.compute_registry_digest()?;
        if self.registry_digest != computed_registry_digest {
            return Err(ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::RegistryDigestMismatch,
                format!(
                    "expected registry digest {}, observed {}",
                    computed_registry_digest, self.registry_digest
                ),
            ));
        }

        let expected_cell_count = catalog.entries.len() * ALL_ACTION_INGRESSES.len();
        if self.cells.len() != expected_cell_count {
            return Err(ActionConsumerRegistryError::new(
                ActionConsumerRegistryErrorCodeV1::MissingCell,
                format!(
                    "expected {expected_cell_count} action×ingress cells, observed {}",
                    self.cells.len()
                ),
            ));
        }

        let declarations = declaration_map(&catalog)?;
        let mut observed = BTreeMap::new();
        for cell in &self.cells {
            let key = (cell.action.clone(), cell.ingress);
            if observed.insert(key.clone(), cell).is_some() {
                return Err(ActionConsumerRegistryError::new(
                    ActionConsumerRegistryErrorCodeV1::DuplicateCell,
                    format!("duplicate matrix cell {} on {:?}", key.0, key.1),
                ));
            }
        }

        let mut declared_cell_count = 0;
        let mut typed_consumer_cell_count = 0;
        for entry in &catalog.entries {
            for ingress in ALL_ACTION_INGRESSES {
                let cell = observed
                    .get(&(entry.action.clone(), ingress))
                    .copied()
                    .ok_or_else(|| {
                        ActionConsumerRegistryError::new(
                            ActionConsumerRegistryErrorCodeV1::MissingCell,
                            format!("missing matrix cell {} on {ingress:?}", entry.action),
                        )
                    })?;
                let declared_ingress = entry.ingresses.contains(&ingress);
                if cell.declared_ingress != declared_ingress
                    || cell.expected_effects != entry.complete_effects
                    || cell.risk_class != entry.risk_class
                    || cell.authority_floor != entry.authority_floor
                {
                    return Err(ActionConsumerRegistryError::new(
                        ActionConsumerRegistryErrorCodeV1::CellCatalogMismatch,
                        format!(
                            "matrix cell {} on {ingress:?} differs from its catalog entry",
                            entry.action
                        ),
                    ));
                }
                let expected = expected_disposition(entry, ingress, &declarations);
                if cell.disposition != expected {
                    return Err(ActionConsumerRegistryError::new(
                        ActionConsumerRegistryErrorCodeV1::DispositionMismatch,
                        format!(
                            "matrix cell {} on {ingress:?} has {:?}, expected {:?}",
                            entry.action, cell.disposition, expected
                        ),
                    ));
                }
                if declared_ingress {
                    declared_cell_count += 1;
                }
                if matches!(
                    cell.disposition,
                    ActionConsumerDispositionV1::EnabledTypedConsumer { .. }
                ) {
                    typed_consumer_cell_count += 1;
                }
            }
        }

        Ok(ActionConsumerRegistryValidationV1 {
            action_count: catalog.entries.len(),
            ingress_count: ALL_ACTION_INGRESSES.len(),
            cell_count: self.cells.len(),
            declared_cell_count,
            typed_consumer_cell_count,
            computed_registry_digest,
        })
    }
}

pub fn m1nd10_action_consumer_registry(
) -> Result<ActionConsumerRegistryV1, ActionConsumerRegistryError> {
    let catalog = m1nd10_action_catalog().map_err(|error| catalog_error(error.to_string()))?;
    catalog
        .validate()
        .map_err(|error| catalog_error(error.to_string()))?;
    if catalog.entries.len() != EXPECTED_M1ND10_ACTION_COUNT
        || catalog.catalog_version != M1ND10_ACTION_CATALOG_VERSION
        || catalog.catalog_digest != EXPECTED_M1ND10_ACTION_CATALOG_DIGEST
    {
        return Err(ActionConsumerRegistryError::new(
            ActionConsumerRegistryErrorCodeV1::CatalogDrift,
            format!(
                "expected {} actions at digest {}, observed {} actions at digest {}",
                EXPECTED_M1ND10_ACTION_COUNT,
                EXPECTED_M1ND10_ACTION_CATALOG_DIGEST,
                catalog.entries.len(),
                catalog.catalog_digest
            ),
        ));
    }
    let declarations = declaration_map(&catalog)?;
    let mut cells = Vec::with_capacity(catalog.entries.len() * ALL_ACTION_INGRESSES.len());
    for entry in &catalog.entries {
        for ingress in ALL_ACTION_INGRESSES {
            cells.push(ActionConsumerCellV1 {
                action: entry.action.clone(),
                ingress,
                declared_ingress: entry.ingresses.contains(&ingress),
                expected_effects: entry.complete_effects.clone(),
                risk_class: entry.risk_class,
                authority_floor: entry.authority_floor,
                disposition: expected_disposition(entry, ingress, &declarations),
            });
        }
    }
    let mut registry = ActionConsumerRegistryV1 {
        schema: ACTION_CONSUMER_REGISTRY_SCHEMA.to_string(),
        registry_version: ACTION_CONSUMER_REGISTRY_VERSION.to_string(),
        action_catalog_version: catalog.catalog_version,
        action_catalog_digest: catalog.catalog_digest,
        cells,
        registry_digest: String::new(),
    };
    registry.seal()?;
    registry.validate()?;
    Ok(registry)
}

pub fn action_consumer_registry_digest() -> Result<String, ActionConsumerRegistryError> {
    Ok(m1nd10_action_consumer_registry()?.registry_digest)
}

fn disabled(
    action: &str,
    ingress: Ingress,
    reason: ConsumerPolicyDisabledReasonV1,
    detail: impl Into<String>,
    registry: Option<&ActionConsumerRegistryV1>,
) -> ConsumerPolicyDisabledV1 {
    ConsumerPolicyDisabledV1 {
        action: action.to_string(),
        ingress,
        reason,
        detail: detail.into(),
        action_catalog_digest: registry.map(|value| value.action_catalog_digest.clone()),
        consumer_registry_digest: registry.map(|value| value.registry_digest.clone()),
    }
}

/// Resolve an exact external typed consumer contract. Generic ORDINARY cells
/// intentionally return `no_exact_consumer`; callers must keep the generic
/// authenticated dispatcher as a separate path.
pub fn external_consumer_contract(
    action: &str,
    ingress: Ingress,
) -> Result<ActionConsumerContractV1, ConsumerPolicyDisabledV1> {
    let registry = m1nd10_action_consumer_registry().map_err(|error| {
        disabled(
            action,
            ingress,
            ConsumerPolicyDisabledReasonV1::NoExactConsumer,
            format!("consumer_registry_invalid: {error}"),
            None,
        )
    })?;
    let Some(cell) = registry.cell(action, ingress) else {
        return Err(disabled(
            action,
            ingress,
            ConsumerPolicyDisabledReasonV1::NotDeclared,
            "action or action×ingress cell is not declared by the pinned catalog",
            Some(&registry),
        ));
    };
    let (consumer_id, contract_version, transport) = match &cell.disposition {
        ActionConsumerDispositionV1::EnabledTypedConsumer {
            consumer_id,
            contract_version,
            transport,
        } => (*consumer_id, contract_version.clone(), *transport),
        ActionConsumerDispositionV1::PolicyDisabled { reason } => {
            return Err(disabled(
                action,
                ingress,
                *reason,
                "policy matrix has no enabled typed consumer for this cell",
                Some(&registry),
            ))
        }
        ActionConsumerDispositionV1::EnabledGenericOrdinary => {
            return Err(disabled(
                action,
                ingress,
                ConsumerPolicyDisabledReasonV1::NoExactConsumer,
                "cell belongs to the separate generic ORDINARY dispatcher",
                Some(&registry),
            ))
        }
        ActionConsumerDispositionV1::EnabledGenericScopedA2Local => {
            return Err(disabled(
                action,
                ingress,
                ConsumerPolicyDisabledReasonV1::NoExactConsumer,
                "cell is admitted A2-LOCAL through the action-keyed generic allowlist; it has no typed lease consumer and none may be inferred",
                Some(&registry),
            ))
        }
    };

    Ok(ActionConsumerContractV1 {
        schema: ACTION_CONSUMER_CONTRACT_SCHEMA.to_string(),
        action: cell.action.clone(),
        ingress: cell.ingress,
        expected_effects: cell.expected_effects.clone(),
        risk_class: cell.risk_class,
        authority_floor: cell.authority_floor,
        consumer_id,
        contract_version,
        transport,
        action_catalog_version: registry.action_catalog_version.clone(),
        action_catalog_digest: registry.action_catalog_digest.clone(),
        consumer_registry_version: registry.registry_version.clone(),
        consumer_registry_digest: registry.registry_digest.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_registry_is_total_digest_bound_and_self_validating() {
        let registry = m1nd10_action_consumer_registry().unwrap();
        let validation = registry.validate().unwrap();
        assert_eq!(validation.action_count, EXPECTED_M1ND10_ACTION_COUNT);
        assert_eq!(validation.ingress_count, 7);
        assert_eq!(
            validation.cell_count,
            EXPECTED_M1ND10_ACTION_COUNT * ALL_ACTION_INGRESSES.len()
        );
        assert_eq!(validation.typed_consumer_cell_count, 18);
        assert_eq!(
            registry.action_catalog_version,
            M1ND10_ACTION_CATALOG_VERSION
        );
        assert_eq!(
            registry.action_catalog_digest,
            EXPECTED_M1ND10_ACTION_CATALOG_DIGEST
        );
        assert_eq!(
            validation.computed_registry_digest,
            registry.registry_digest
        );
        assert_eq!(
            action_consumer_registry_digest().unwrap(),
            registry.registry_digest
        );
    }

    /// The generic door opens for EXACTLY the ratified action and nothing else,
    /// and it opens at exactly the ratified floor
    /// (`docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §6 items 1–2, owner 2026-07-29).
    /// A second entry appearing here without a second ratification is the failure
    /// this test exists to make loud.
    #[test]
    fn the_a2_local_allowlist_holds_exactly_the_ratified_action() {
        assert_eq!(
            GENERIC_A2_LOCAL_ADMITTED_ACTIONS,
            ["graph.ingest.refresh_declared_root"]
        );
        let catalog = m1nd10_action_catalog().unwrap();
        for action in GENERIC_A2_LOCAL_ADMITTED_ACTIONS {
            let entry = catalog
                .entries
                .iter()
                .find(|entry| entry.action.as_str() == *action)
                .unwrap_or_else(|| panic!("{action} is allowlisted but absent from the catalog"));
            assert_eq!(entry.authority_floor, AuthorityFloor::ScopedGrantA2);
            assert!(entry.ingresses.contains(&Ingress::Mcp));
            // Never sovereign: a door that could change roots or cross brains
            // does not belong on a list that skips the lease plane.
            assert!(!entry.complete_effects.contains(&Effect::SovereignMutation));
        }
        // The two siblings at the SAME floor must NOT be on it (verdict RC-4).
        for sibling in ["source.edit.commit", "graph.ingest.merge_existing"] {
            assert!(
                !GENERIC_A2_LOCAL_ADMITTED_ACTIONS.contains(&sibling),
                "{sibling} shares the floor but was never ratified through the generic door"
            );
        }
    }

    #[test]
    fn declared_and_undeclared_ingresses_have_closed_exact_dispositions() {
        let catalog = m1nd10_action_catalog().unwrap();
        let registry = m1nd10_action_consumer_registry().unwrap();
        for entry in catalog.entries {
            for ingress in ALL_ACTION_INGRESSES {
                let cell = registry.cell(entry.action.as_str(), ingress).unwrap();
                assert_eq!(cell.declared_ingress, entry.ingresses.contains(&ingress));
                if !entry.ingresses.contains(&ingress) {
                    assert_eq!(
                        cell.disposition,
                        ActionConsumerDispositionV1::PolicyDisabled {
                            reason: ConsumerPolicyDisabledReasonV1::NotDeclared
                        }
                    );
                }
                if matches!(
                    cell.disposition,
                    ActionConsumerDispositionV1::EnabledGenericOrdinary
                ) {
                    assert_eq!(entry.authority_floor, AuthorityFloor::Ordinary);
                    assert!(entry.ingresses.contains(&ingress));
                }
                // The A2-local opening is CLOSED and named. Any cell carrying it
                // must be one of the explicitly listed actions, must be at the
                // ratified `ScopedGrantA2` floor, and must be on a declared
                // ingress — an unnamed action can never acquire it by drift.
                if matches!(
                    cell.disposition,
                    ActionConsumerDispositionV1::EnabledGenericScopedA2Local
                ) {
                    assert!(
                        GENERIC_A2_LOCAL_ADMITTED_ACTIONS.contains(&entry.action.as_str()),
                        "{} is A2-local without being on the allowlist",
                        entry.action.as_str()
                    );
                    assert_eq!(entry.authority_floor, AuthorityFloor::ScopedGrantA2);
                    assert!(entry.ingresses.contains(&ingress));
                }
            }
        }
    }

    #[test]
    fn typed_consumer_set_is_exact_and_no_disabled_cell_is_claimed() {
        let registry = m1nd10_action_consumer_registry().unwrap();
        let typed: BTreeSet<(String, Ingress, TypedConsumerIdV1)> = registry
            .cells
            .iter()
            .filter_map(|cell| match cell.disposition {
                ActionConsumerDispositionV1::EnabledTypedConsumer { consumer_id, .. } => {
                    Some((cell.action.to_string(), cell.ingress, consumer_id))
                }
                _ => None,
            })
            .collect();

        let mut expected = BTreeSet::new();
        for ingress in [Ingress::Mcp, Ingress::Rest] {
            for action in [
                "mission.service.land_intent",
                "mission.service.mission_transition",
                "mission.service.execution_dispatch",
                "mission.service.execution_started",
                "mission.service.execution_terminal",
                "mission.service.land",
            ] {
                expected.insert((
                    action.to_string(),
                    ingress,
                    TypedConsumerIdV1::MissionService,
                ));
            }
        }
        for action in [
            "system_blocks.ratify",
            "brain.promote",
            "source.edit.commit",
            "graph.ingest.preview",
            "graph.ingest.replace",
            "graph.ingest.merge_existing",
        ] {
            expected.insert((
                action.to_string(),
                Ingress::Mcp,
                TypedConsumerIdV1::ExternalMutationService,
            ));
        }
        assert_eq!(typed, expected);
    }

    #[test]
    fn external_lookup_returns_exact_binding_or_a_closed_refusal() {
        let contract = external_consumer_contract("brain.promote", Ingress::Mcp).unwrap();
        assert_eq!(contract.action.as_str(), "brain.promote");
        assert_eq!(contract.ingress, Ingress::Mcp);
        assert_eq!(
            contract.consumer_id,
            TypedConsumerIdV1::ExternalMutationService
        );
        assert_eq!(
            contract.contract_version,
            EXTERNAL_MUTATION_SERVICE_CONTRACT_VERSION
        );
        assert_eq!(
            contract.transport,
            TypedConsumerTransportV1::McpStreamableHttp
        );
        assert_eq!(
            contract.expected_effects,
            effects([
                Effect::GraphMutation,
                Effect::RuntimeStoreWrite,
                Effect::HostFilesystemWrite,
                Effect::SovereignMutation,
            ])
        );
        assert_eq!(
            contract.action_catalog_digest,
            EXPECTED_M1ND10_ACTION_CATALOG_DIGEST
        );
        assert!(!contract.consumer_registry_digest.is_empty());

        let replace = external_consumer_contract("graph.ingest.replace", Ingress::Mcp).unwrap();
        assert_eq!(replace.authority_floor, AuthorityFloor::PositiveSovereign);
        assert_eq!(replace.risk_class, RiskClass::Critical);
        assert_eq!(
            replace.expected_effects,
            effects([
                Effect::GraphMutation,
                Effect::RuntimeStoreWrite,
                Effect::SovereignMutation,
            ])
        );
        let merge =
            external_consumer_contract("graph.ingest.merge_existing", Ingress::Mcp).unwrap();
        assert_eq!(merge.authority_floor, AuthorityFloor::ScopedGrantA2);
        assert_eq!(merge.risk_class, RiskClass::High);
        assert_eq!(
            merge.expected_effects,
            effects([Effect::GraphMutation, Effect::RuntimeStoreWrite])
        );

        let rest = external_consumer_contract("brain.promote", Ingress::Rest).unwrap_err();
        assert_eq!(rest.reason, ConsumerPolicyDisabledReasonV1::NotDeclared);
        let registry = m1nd10_action_consumer_registry().unwrap();
        let generic_cell = registry
            .cells
            .iter()
            .find(|cell| {
                matches!(
                    cell.disposition,
                    ActionConsumerDispositionV1::EnabledGenericOrdinary
                )
            })
            .unwrap();
        let generic =
            external_consumer_contract(generic_cell.action.as_str(), generic_cell.ingress)
                .unwrap_err();
        assert_eq!(
            generic.reason,
            ConsumerPolicyDisabledReasonV1::NoExactConsumer
        );
        let unknown = external_consumer_contract("unknown.action", Ingress::Mcp).unwrap_err();
        assert_eq!(unknown.reason, ConsumerPolicyDisabledReasonV1::NotDeclared);
    }

    #[test]
    fn elevated_cells_never_use_generic_and_safety_never_uses_positive_consumer() {
        let registry = m1nd10_action_consumer_registry().unwrap();
        for cell in registry.cells {
            if cell.authority_floor != AuthorityFloor::Ordinary {
                assert!(!matches!(
                    cell.disposition,
                    ActionConsumerDispositionV1::EnabledGenericOrdinary
                ));
            }
            if cell.authority_floor == AuthorityFloor::SafetyOnly {
                assert!(!matches!(
                    cell.disposition,
                    ActionConsumerDispositionV1::EnabledTypedConsumer {
                        consumer_id: TypedConsumerIdV1::ExternalMutationService,
                        ..
                    }
                ));
            }
        }
    }

    #[test]
    fn drift_holes_and_duplicates_fail_closed() {
        let mut drift = m1nd10_action_consumer_registry().unwrap();
        drift.action_catalog_digest = "0".repeat(64);
        drift.seal().unwrap();
        assert_eq!(
            drift.validate().unwrap_err().code,
            ActionConsumerRegistryErrorCodeV1::CatalogDrift
        );

        let mut hole = m1nd10_action_consumer_registry().unwrap();
        hole.cells.pop();
        hole.seal().unwrap();
        assert_eq!(
            hole.validate().unwrap_err().code,
            ActionConsumerRegistryErrorCodeV1::MissingCell
        );

        let mut duplicate = m1nd10_action_consumer_registry().unwrap();
        let last = duplicate.cells.last().unwrap().clone();
        duplicate.cells.push(last);
        duplicate.seal().unwrap();
        assert_eq!(
            duplicate.validate().unwrap_err().code,
            ActionConsumerRegistryErrorCodeV1::MissingCell
        );
        duplicate.cells.pop();
        let first = duplicate.cells.first().unwrap().clone();
        duplicate.cells[1] = first;
        duplicate.seal().unwrap();
        assert_eq!(
            duplicate.validate().unwrap_err().code,
            ActionConsumerRegistryErrorCodeV1::DuplicateCell
        );
    }

    #[test]
    fn declarations_reject_duplicate_floor_effect_risk_and_authority_domain_mismatch() {
        let catalog = m1nd10_action_catalog().unwrap();
        let declarations = typed_consumer_declarations();
        validate_typed_consumer_declarations(&catalog, &declarations).unwrap();

        let mut duplicate = declarations.clone();
        duplicate.push(declarations[0].clone());
        assert_eq!(
            validate_typed_consumer_declarations(&catalog, &duplicate)
                .unwrap_err()
                .code,
            ActionConsumerRegistryErrorCodeV1::DuplicateConsumerDeclaration
        );

        let promote_index = declarations
            .iter()
            .position(|value| value.action == "brain.promote")
            .unwrap();
        let mut wrong_floor = declarations.clone();
        wrong_floor[promote_index].expected_authority_floor = AuthorityFloor::ScopedGrantA2;
        assert_eq!(
            validate_typed_consumer_declarations(&catalog, &wrong_floor)
                .unwrap_err()
                .code,
            ActionConsumerRegistryErrorCodeV1::ConsumerFloorMismatch
        );

        let mut wrong_effects = declarations.clone();
        wrong_effects[promote_index].expected_effects = effects([Effect::SovereignMutation]);
        assert_eq!(
            validate_typed_consumer_declarations(&catalog, &wrong_effects)
                .unwrap_err()
                .code,
            ActionConsumerRegistryErrorCodeV1::ConsumerEffectsMismatch
        );

        let mut wrong_risk = declarations.clone();
        wrong_risk[promote_index].expected_risk_class = RiskClass::High;
        assert_eq!(
            validate_typed_consumer_declarations(&catalog, &wrong_risk)
                .unwrap_err()
                .code,
            ActionConsumerRegistryErrorCodeV1::ConsumerRiskMismatch
        );

        let mut wrong_transport = declarations.clone();
        wrong_transport[promote_index].transport = TypedConsumerTransportV1::Rest;
        assert_eq!(
            validate_typed_consumer_declarations(&catalog, &wrong_transport)
                .unwrap_err()
                .code,
            ActionConsumerRegistryErrorCodeV1::ConsumerIngressMismatch
        );

        let safety = catalog
            .entries
            .iter()
            .find(|entry| entry.authority_floor == AuthorityFloor::SafetyOnly)
            .unwrap();
        let mut safety_positive = declarations.clone();
        safety_positive.push(TypedConsumerDeclarationV1 {
            action: safety.action.to_string(),
            ingress: *safety.ingresses.iter().next().unwrap(),
            expected_effects: safety.complete_effects.clone(),
            expected_risk_class: safety.risk_class,
            expected_authority_floor: safety.authority_floor,
            consumer_id: TypedConsumerIdV1::ExternalMutationService,
            contract_version: EXTERNAL_MUTATION_SERVICE_CONTRACT_VERSION,
            transport: TypedConsumerTransportV1::McpStreamableHttp,
        });
        assert_eq!(
            validate_typed_consumer_declarations(&catalog, &safety_positive)
                .unwrap_err()
                .code,
            ActionConsumerRegistryErrorCodeV1::ConsumerAuthorityDomainMismatch
        );
    }

    #[test]
    fn disposition_wire_values_are_closed_and_explicit() {
        assert_eq!(
            serde_json::to_value(ActionConsumerDispositionV1::EnabledGenericOrdinary).unwrap(),
            serde_json::json!({"disposition": "enabled_generic_ordinary"})
        );
        assert_eq!(
            serde_json::to_value(ActionConsumerDispositionV1::EnabledTypedConsumer {
                consumer_id: TypedConsumerIdV1::ExternalMutationService,
                contract_version: "v1".to_string(),
                transport: TypedConsumerTransportV1::McpStreamableHttp,
            })
            .unwrap(),
            serde_json::json!({
                "disposition": "enabled_typed_consumer",
                "consumer_id": "external_mutation_service",
                "contract_version": "v1",
                "transport": "mcp_streamable_http"
            })
        );
        assert_eq!(
            serde_json::to_value(ActionConsumerDispositionV1::PolicyDisabled {
                reason: ConsumerPolicyDisabledReasonV1::NotDeclared,
            })
            .unwrap(),
            serde_json::json!({
                "disposition": "policy_disabled",
                "reason": "not_declared"
            })
        );
        assert!(
            serde_json::from_value::<ActionConsumerDispositionV1>(serde_json::json!({
                "disposition": "policy_disabled",
                "reason": "not_declared",
                "undeclared_escape": true
            }))
            .is_err()
        );
    }
}
