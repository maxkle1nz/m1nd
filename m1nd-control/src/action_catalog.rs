use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::policy::{ActionId, Effect, Ingress, RiskClass};
use crate::{digest_canonical, CanonicalError};

pub const ACTION_CATALOG_SCHEMA: &str = "m1nd-action-catalog-v1";
pub const ACTION_CATALOG_DIGEST_DOMAIN: &str = "m1nd-action-catalog-v1";
pub const M1ND10_ACTION_CATALOG_VERSION: &str = "m1nd10-g2-2026-07-18.4";

/// The minimum positive authority path an action may use.
///
/// `SafetyOnly` is deliberately not a positive authority. It can only select
/// immutable negative safety effects and can never authorize an ordinary or
/// sovereign mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorityFloor {
    Ordinary,
    ScopedGrantA2,
    PositiveSovereign,
    ServiceIdentity,
    SafetyOnly,
}

/// One semantic action, independent of the transport method that reaches it.
///
/// `complete_effects` contains both direct effects and effects reachable
/// transitively through helpers, subprocesses, hooks, or background work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionCatalogEntryV1 {
    pub action: ActionId,
    pub ingresses: BTreeSet<Ingress>,
    pub complete_effects: BTreeSet<Effect>,
    pub risk_class: RiskClass,
    pub authority_floor: AuthorityFloor,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionCatalogV1 {
    pub schema: String,
    pub catalog_version: String,
    pub entries: Vec<ActionCatalogEntryV1>,
    pub catalog_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionCatalogValidation {
    pub entry_count: usize,
    pub action_count: usize,
    pub ingress_count: usize,
    pub computed_catalog_digest: String,
}

#[derive(Debug, Error)]
pub enum ActionCatalogError {
    #[error("unsupported action catalog schema '{actual}'")]
    Schema { actual: String },
    #[error("required field '{field}' is empty")]
    EmptyRequired { field: &'static str },
    #[error("action catalog must declare at least one entry")]
    NoEntries,
    #[error("action id '{action}' is not a lowercase dotted semantic id")]
    InvalidSemanticActionId { action: ActionId },
    #[error("duplicate action catalog entry '{action}'")]
    DuplicateAction { action: ActionId },
    #[error("catalog entries are not strictly sorted: '{previous}' precedes '{current}'")]
    EntriesNotStrictlySorted {
        previous: ActionId,
        current: ActionId,
    },
    #[error("action '{action}' has no reachable ingress")]
    EmptyIngresses { action: ActionId },
    #[error("action '{action}' has no complete effects")]
    EmptyEffects { action: ActionId },
    #[error("SAFETY_ONLY action '{action}' contains non-safety effect {effect:?}")]
    NonSafetyEffectInSafetyAction { action: ActionId, effect: Effect },
    #[error("non-safety action '{action}' contains negative safety effect {effect:?}")]
    SafetyEffectOutsideSafetyAction { action: ActionId, effect: Effect },
    #[error("action '{action}' contains SOVEREIGN_MUTATION without POSITIVE_SOVEREIGN floor")]
    SovereignEffectWithoutPositiveFloor { action: ActionId },
    #[error("POSITIVE_SOVEREIGN action '{action}' omits SOVEREIGN_MUTATION")]
    PositiveFloorWithoutSovereignEffect { action: ActionId },
    #[error("POSITIVE_SOVEREIGN action '{action}' must be CRITICAL risk")]
    PositiveSovereignMustBeCritical { action: ActionId },
    #[error("SAFETY_ONLY action '{action}' must be CRITICAL risk")]
    SafetyOnlyMustBeCritical { action: ActionId },
    #[error("CRITICAL action '{action}' requires POSITIVE_SOVEREIGN or SAFETY_ONLY floor")]
    CriticalActionHasInsufficientFloor { action: ActionId },
    #[error("ORDINARY action '{action}' contains elevated effect {effect:?}")]
    ElevatedEffectInOrdinaryAction { action: ActionId, effect: Effect },
    #[error("EXECUTABLE_REPLACEMENT action '{action}' requires POSITIVE_SOVEREIGN floor")]
    ExecutableReplacementWithoutPositiveFloor { action: ActionId },
    #[error("SERVICE_IDENTITY action '{action}' has no service-capable ingress")]
    ServiceIdentityWithoutServiceIngress { action: ActionId },
    #[error("catalog digest mismatch: expected {expected}, observed {observed}")]
    CatalogDigestMismatch { expected: String, observed: String },
    #[error(transparent)]
    Canonical(#[from] CanonicalError),
}

impl ActionCatalogV1 {
    /// Compute the catalog self-hash while omitting only `catalog_digest`.
    pub fn compute_catalog_digest(&self) -> Result<String, CanonicalError> {
        let mut value = serde_json::to_value(self)?;
        let object = value
            .as_object_mut()
            .expect("ActionCatalogV1 always serializes as an object");
        object.remove("catalog_digest");
        digest_canonical(ACTION_CATALOG_DIGEST_DOMAIN, &value)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.catalog_digest = self.compute_catalog_digest()?;
        Ok(())
    }

    /// Validate deterministic shape and authority/effect separation without
    /// executing or authorizing any action.
    pub fn validate(&self) -> Result<ActionCatalogValidation, ActionCatalogError> {
        if self.schema != ACTION_CATALOG_SCHEMA {
            return Err(ActionCatalogError::Schema {
                actual: self.schema.clone(),
            });
        }
        require_non_empty("catalog_version", &self.catalog_version)?;
        require_non_empty("catalog_digest", &self.catalog_digest)?;
        if self.entries.is_empty() {
            return Err(ActionCatalogError::NoEntries);
        }

        let mut actions = BTreeMap::new();
        let mut all_ingresses = BTreeSet::new();
        let mut previous: Option<&ActionId> = None;
        for entry in &self.entries {
            validate_semantic_action_id(&entry.action)?;
            if entry.ingresses.is_empty() {
                return Err(ActionCatalogError::EmptyIngresses {
                    action: entry.action.clone(),
                });
            }
            if entry.complete_effects.is_empty() {
                return Err(ActionCatalogError::EmptyEffects {
                    action: entry.action.clone(),
                });
            }
            if actions.insert(entry.action.clone(), ()).is_some() {
                return Err(ActionCatalogError::DuplicateAction {
                    action: entry.action.clone(),
                });
            }
            if let Some(previous) = previous {
                if previous >= &entry.action {
                    return Err(ActionCatalogError::EntriesNotStrictlySorted {
                        previous: previous.clone(),
                        current: entry.action.clone(),
                    });
                }
            }
            previous = Some(&entry.action);
            all_ingresses.extend(entry.ingresses.iter().copied());
            validate_entry_authority(entry)?;
        }

        let computed_catalog_digest = self.compute_catalog_digest()?;
        if self.catalog_digest != computed_catalog_digest {
            return Err(ActionCatalogError::CatalogDigestMismatch {
                expected: computed_catalog_digest,
                observed: self.catalog_digest.clone(),
            });
        }

        Ok(ActionCatalogValidation {
            entry_count: self.entries.len(),
            action_count: actions.len(),
            ingress_count: all_ingresses.len(),
            computed_catalog_digest,
        })
    }
}

fn validate_semantic_action_id(action: &ActionId) -> Result<(), ActionCatalogError> {
    if !action.is_semantic_catalog_id() {
        return Err(ActionCatalogError::InvalidSemanticActionId {
            action: action.clone(),
        });
    }
    Ok(())
}

fn validate_entry_authority(entry: &ActionCatalogEntryV1) -> Result<(), ActionCatalogError> {
    let first_safety_effect = entry
        .complete_effects
        .iter()
        .find(|effect| effect.is_negative_safety())
        .copied();
    match entry.authority_floor {
        AuthorityFloor::SafetyOnly => {
            if let Some(effect) = entry
                .complete_effects
                .iter()
                .find(|effect| !effect.is_negative_safety())
            {
                return Err(ActionCatalogError::NonSafetyEffectInSafetyAction {
                    action: entry.action.clone(),
                    effect: *effect,
                });
            }
            if entry.risk_class != RiskClass::Critical {
                return Err(ActionCatalogError::SafetyOnlyMustBeCritical {
                    action: entry.action.clone(),
                });
            }
        }
        AuthorityFloor::PositiveSovereign => {
            if let Some(effect) = first_safety_effect {
                return Err(ActionCatalogError::SafetyEffectOutsideSafetyAction {
                    action: entry.action.clone(),
                    effect,
                });
            }
            if !entry.complete_effects.contains(&Effect::SovereignMutation) {
                return Err(ActionCatalogError::PositiveFloorWithoutSovereignEffect {
                    action: entry.action.clone(),
                });
            }
            if entry.risk_class != RiskClass::Critical {
                return Err(ActionCatalogError::PositiveSovereignMustBeCritical {
                    action: entry.action.clone(),
                });
            }
        }
        floor => {
            if let Some(effect) = first_safety_effect {
                return Err(ActionCatalogError::SafetyEffectOutsideSafetyAction {
                    action: entry.action.clone(),
                    effect,
                });
            }
            if entry.complete_effects.contains(&Effect::SovereignMutation) {
                return Err(ActionCatalogError::SovereignEffectWithoutPositiveFloor {
                    action: entry.action.clone(),
                });
            }
            if entry.risk_class == RiskClass::Critical {
                return Err(ActionCatalogError::CriticalActionHasInsufficientFloor {
                    action: entry.action.clone(),
                });
            }
            if floor == AuthorityFloor::Ordinary {
                if let Some(effect) = entry
                    .complete_effects
                    .iter()
                    .find(|effect| effect_requires_elevated_authority(**effect))
                {
                    return Err(ActionCatalogError::ElevatedEffectInOrdinaryAction {
                        action: entry.action.clone(),
                        effect: *effect,
                    });
                }
            }
            if floor == AuthorityFloor::ServiceIdentity
                && !entry.ingresses.iter().any(|ingress| {
                    matches!(
                        ingress,
                        Ingress::Rest
                            | Ingress::Cli
                            | Ingress::Hook
                            | Ingress::BackgroundJob
                            | Ingress::Recovery
                            | Ingress::Migration
                    )
                })
            {
                return Err(ActionCatalogError::ServiceIdentityWithoutServiceIngress {
                    action: entry.action.clone(),
                });
            }
        }
    }

    if entry
        .complete_effects
        .contains(&Effect::ExecutableReplacement)
        && entry.authority_floor != AuthorityFloor::PositiveSovereign
    {
        return Err(
            ActionCatalogError::ExecutableReplacementWithoutPositiveFloor {
                action: entry.action.clone(),
            },
        );
    }
    Ok(())
}

const fn effect_requires_elevated_authority(effect: Effect) -> bool {
    matches!(
        effect,
        Effect::SourceFilesystemWrite
            | Effect::HostFilesystemWrite
            | Effect::ProcessSpawn
            | Effect::ProcessSignal
            | Effect::ExecutableReplacement
            | Effect::NetworkAccess
            | Effect::NetworkExpose
    )
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), ActionCatalogError> {
    if value.trim().is_empty() {
        return Err(ActionCatalogError::EmptyRequired { field });
    }
    Ok(())
}

fn entry<const I: usize, const E: usize>(
    action: &str,
    ingresses: [Ingress; I],
    complete_effects: [Effect; E],
    risk_class: RiskClass,
    authority_floor: AuthorityFloor,
) -> ActionCatalogEntryV1 {
    ActionCatalogEntryV1 {
        action: ActionId::new(action).expect("built-in action ids are non-empty"),
        ingresses: ingresses.into_iter().collect(),
        complete_effects: complete_effects.into_iter().collect(),
        risk_class,
        authority_floor,
    }
}

/// Canonical G2 inventory of every audited action that can mutate owner state,
/// source/host files, processes, or durable coordination. Read-like variants
/// are retained when they have a mutating ledger/cache side effect or when an
/// input-sensitive tool must be split fail-closed from its mutating variant.
pub fn m1nd10_action_catalog() -> Result<ActionCatalogV1, ActionCatalogError> {
    use AuthorityFloor::{Ordinary, PositiveSovereign, SafetyOnly, ScopedGrantA2, ServiceIdentity};
    use Effect::{
        AbortPrepared, CoordinationRecord, DemoteGrant, EpochBump, EpochFence,
        ExecutableReplacement, FreezeIssuance, GraphMutation, HostFilesystemWrite,
        MissionStateWrite, NetworkAccess, NetworkExpose, ProcessSignal, ProcessSpawn, Read,
        RevokeCapability, RollbackSignedCandidate, RuntimeStoreWrite, SourceFilesystemWrite,
        SovereignMutation,
    };
    use Ingress::{BackgroundJob, Cli, Hook, Mcp, Migration, Recovery, Rest};
    use RiskClass::{Critical, High, Low, Medium};

    let mut entries = vec![
        // The broker-issued lease is only an authorization artifact. It does
        // not perform the target action, and the target is re-authorized and
        // consumed separately at its own exact policy tuple.
        entry(
            "authority.authorize",
            [Mcp, Rest],
            [RuntimeStoreWrite, CoordinationRecord],
            High,
            Ordinary,
        ),
        // MCP/REST intercepts and graph/store mutations.
        entry(
            "brain.bootstrap",
            [Mcp, Rest],
            [
                GraphMutation,
                RuntimeStoreWrite,
                HostFilesystemWrite,
                SovereignMutation,
            ],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "brain.promote",
            [Mcp],
            [
                GraphMutation,
                RuntimeStoreWrite,
                HostFilesystemWrite,
                SovereignMutation,
            ],
            Critical,
            PositiveSovereign,
        ),
        entry("graph.ingest.preview", [Mcp], [Read], Low, Ordinary),
        entry(
            "graph.ingest.merge_existing",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite],
            High,
            ScopedGrantA2,
        ),
        // The freshness door (GENESIS-INGEST-CONSUMERS-SPEC.md §1, owner-ratified
        // 2026-07-29). It re-scans a root the bound brain has ALREADY declared:
        // same effects as `merge_existing`, and deliberately NO `SovereignMutation`,
        // because it structurally cannot change the root set or cross to another
        // brain's territory. Its floor is the ratified `ScopedGrantA2`.
        entry(
            "graph.ingest.refresh_declared_root",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite],
            High,
            ScopedGrantA2,
        ),
        entry(
            "graph.ingest.change_roots",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "graph.ingest.replace",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "graph.audit.replace",
            [Mcp, Hook],
            [GraphMutation, RuntimeStoreWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "graph.federate.replace",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        entry("graph.federate_auto.preview", [Mcp], [Read], Low, Ordinary),
        entry(
            "graph.federate_auto.execute",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "graph.learn",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite],
            High,
            ScopedGrantA2,
        ),
        // Nominal reads with plasticity, findings, counters, or persistence.
        entry(
            "query.activate",
            [Mcp],
            [Read, GraphMutation, RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        entry(
            "query.missing",
            [Mcp],
            [Read, GraphMutation, RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        entry(
            "query.orient",
            [Mcp],
            [Read, GraphMutation, RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "query.north",
            [Mcp],
            [Read, GraphMutation, RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "query.seek",
            [Mcp],
            [Read, GraphMutation, RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        entry(
            "query.scan",
            [Mcp],
            [Read, RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "query.scan_all",
            [Mcp],
            [Read, RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "query.taint_trace",
            [Mcp],
            [Read, RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        entry(
            "query.twins",
            [Mcp],
            [Read, RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        entry(
            "query.refactor_plan",
            [Mcp],
            [Read, RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        // Pure MCP reads share one semantic floor. The transport parity layer
        // maps every such tool explicitly; this entry is not an unknown-tool
        // fallback and cannot authorize a mutation.
        entry("query.read", [Mcp], [Read], Low, Ordinary),
        // G5 EvidenceQuery is explicit because its no-write guarantee is part of
        // the contract: REST and Streamable MCP verify the committed projection
        // prefix without lock creation, tail repair, or cache mutation.
        entry("evidence.query", [Mcp, Rest], [Read], Low, Ordinary),
        // Persistence, memory, and universal-document cache paths.
        entry("store.persist.status", [Mcp], [Read], Low, Ordinary),
        entry(
            "store.persist.save_runtime",
            [Mcp],
            [RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        entry(
            "store.persist.save_explicit_path",
            [Mcp],
            [HostFilesystemWrite],
            High,
            ScopedGrantA2,
        ),
        entry(
            "store.persist.checkpoint",
            [Mcp],
            [RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        entry(
            "store.persist.load_replace",
            [Mcp, Recovery],
            [Read, GraphMutation, RuntimeStoreWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "memory.memorize.default",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        entry(
            "memory.memorize.explicit_path",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite, HostFilesystemWrite],
            High,
            ScopedGrantA2,
        ),
        entry("boot_memory.read", [Mcp], [Read], Low, Ordinary),
        entry(
            "boot_memory.set",
            [Mcp],
            [RuntimeStoreWrite],
            Medium,
            ScopedGrantA2,
        ),
        entry(
            "boot_memory.delete",
            [Mcp],
            [RuntimeStoreWrite],
            Medium,
            ScopedGrantA2,
        ),
        entry(
            "documents.resolve.refresh_cache",
            [Mcp],
            [Read, RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        entry(
            "documents.bindings.refresh_cache",
            [Mcp],
            [Read, RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        entry(
            "documents.drift.refresh_cache",
            [Mcp],
            [Read, RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        // Surgical and XRay variants remain distinct by commit semantics.
        entry(
            "source.apply.single",
            [Mcp],
            [SourceFilesystemWrite, RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "source.apply.batch",
            [Mcp],
            [SourceFilesystemWrite, RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "source.edit.commit",
            [Mcp],
            [SourceFilesystemWrite, RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "source.edit.preview",
            [Mcp],
            [Read, CoordinationRecord],
            Low,
            Ordinary,
        ),
        entry(
            "source.surgical_context.mark_proof_ready",
            [Mcp],
            [Read, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        // The transplant verb moves a top-level item across files, writing
        // source + dest + every derived referencer. Its
        // effect tuple mirrors source.apply/source.edit.commit — a real on-disk
        // source write consumed under the armed proof gate. `transplant_commit`
        // lands a staged plan (the same write under a handle); `transplant_preview`
        // only stages in memory, so it carries the read stance of source.edit.preview.
        entry(
            "source.transplant.single",
            [Mcp],
            [SourceFilesystemWrite, RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "source.transplant.commit",
            [Mcp],
            [SourceFilesystemWrite, RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "source.transplant.preview",
            [Mcp],
            [Read, CoordinationRecord],
            Low,
            Ordinary,
        ),
        entry("xray.apply.dry_run", [Mcp], [Read], Medium, Ordinary),
        entry(
            "xray.apply.commit",
            [Mcp],
            [SourceFilesystemWrite, RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "xray.retag.dry_run",
            [Mcp],
            [Read, RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "xray.retag.commit",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "xray.paint.dry_run",
            [Mcp],
            [Read, RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "xray.paint.commit",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        // System-block governance and candidate lifecycle.
        entry(
            "system_blocks.seed_import.force",
            [Mcp],
            [RuntimeStoreWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "system_blocks.skeleton_candidate",
            [Mcp],
            [
                RuntimeStoreWrite,
                CoordinationRecord,
                ProcessSpawn,
                HostFilesystemWrite,
                NetworkAccess,
            ],
            High,
            ScopedGrantA2,
        ),
        entry(
            "system_blocks.candidate_naming",
            [Mcp, Rest],
            [
                RuntimeStoreWrite,
                CoordinationRecord,
                ProcessSpawn,
                HostFilesystemWrite,
                NetworkAccess,
            ],
            High,
            ScopedGrantA2,
        ),
        entry(
            "system_blocks.ratify",
            [Mcp],
            [RuntimeStoreWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "system_blocks.receipt_import",
            [Mcp],
            [RuntimeStoreWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "system_blocks.reconcile",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "system_blocks.archive",
            [Mcp],
            [RuntimeStoreWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "system_blocks.restore",
            [Mcp, Recovery],
            [RuntimeStoreWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "system_blocks.delete.permanent",
            [Mcp],
            [RuntimeStoreWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "system_blocks.candidate_edit",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "system_blocks.lease.acquire",
            [Mcp],
            [CoordinationRecord, RuntimeStoreWrite],
            Low,
            Ordinary,
        ),
        entry(
            "system_blocks.lease.refresh",
            [Mcp],
            [CoordinationRecord, RuntimeStoreWrite],
            Low,
            Ordinary,
        ),
        entry(
            "system_blocks.lease.release",
            [Mcp],
            [CoordinationRecord, RuntimeStoreWrite],
            Low,
            Ordinary,
        ),
        // Mission state, letters, delegation, and runner jobs.
        entry(
            "mission.start",
            [Mcp],
            [MissionStateWrite, RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        entry(
            "mission.event",
            [Mcp],
            [MissionStateWrite, RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        entry(
            "mission.next",
            [Mcp],
            [MissionStateWrite, RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        entry(
            "mission.verify",
            [Mcp],
            [MissionStateWrite, RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "mission.handoff",
            [Mcp],
            [MissionStateWrite, RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "mission.close",
            [Mcp],
            [MissionStateWrite, RuntimeStoreWrite],
            Medium,
            Ordinary,
        ),
        entry(
            "mission.close_with_memory",
            [Mcp],
            [MissionStateWrite, RuntimeStoreWrite, GraphMutation],
            High,
            ScopedGrantA2,
        ),
        entry(
            "mission.post.ordinary",
            [Mcp],
            [MissionStateWrite, RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "mission.post.landed",
            [Mcp],
            [
                MissionStateWrite,
                RuntimeStoreWrite,
                CoordinationRecord,
                SovereignMutation,
            ],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "mission.post.archive",
            [Mcp],
            [
                MissionStateWrite,
                RuntimeStoreWrite,
                CoordinationRecord,
                SovereignMutation,
            ],
            Critical,
            PositiveSovereign,
        ),
        // M1ND-10 G3: the versioned MissionService is the sole advertised
        // external mission mutation boundary. The older post/import actions
        // remain catalogued only as denied compatibility tombstones.
        entry(
            "mission.service.land_intent",
            [Mcp, Rest],
            [Read],
            Low,
            Ordinary,
        ),
        entry(
            "mission.service.mission_transition",
            [Mcp, Rest],
            [MissionStateWrite, RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "mission.service.execution_dispatch",
            [Mcp, Rest],
            [MissionStateWrite, RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "mission.service.execution_started",
            [Mcp, Rest],
            [MissionStateWrite, RuntimeStoreWrite, CoordinationRecord],
            High,
            ServiceIdentity,
        ),
        entry(
            "mission.service.execution_terminal",
            [Mcp, Rest],
            [MissionStateWrite, RuntimeStoreWrite, CoordinationRecord],
            High,
            ServiceIdentity,
        ),
        entry(
            "mission.service.land",
            [Mcp, Rest],
            [
                MissionStateWrite,
                RuntimeStoreWrite,
                CoordinationRecord,
                SovereignMutation,
            ],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "mission.spawn",
            [Mcp, Rest],
            [
                MissionStateWrite,
                RuntimeStoreWrite,
                CoordinationRecord,
                HostFilesystemWrite,
                ProcessSpawn,
                NetworkAccess,
            ],
            High,
            ScopedGrantA2,
        ),
        entry(
            "mission.curation_spawn",
            [Rest],
            [
                RuntimeStoreWrite,
                CoordinationRecord,
                HostFilesystemWrite,
                ProcessSpawn,
                NetworkAccess,
            ],
            High,
            ScopedGrantA2,
        ),
        entry(
            "delegation.delegate",
            [Mcp],
            [Read, RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "delegation.debrief",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "runner.mission.execute",
            [Rest, BackgroundJob],
            [
                SourceFilesystemWrite,
                HostFilesystemWrite,
                RuntimeStoreWrite,
                MissionStateWrite,
                CoordinationRecord,
                ProcessSpawn,
                NetworkAccess,
            ],
            High,
            ServiceIdentity,
        ),
        entry(
            "runner.naming.execute",
            [Rest, BackgroundJob],
            [
                HostFilesystemWrite,
                RuntimeStoreWrite,
                CoordinationRecord,
                ProcessSpawn,
                NetworkAccess,
            ],
            High,
            ServiceIdentity,
        ),
        entry(
            "runner.curation.execute",
            [Rest, BackgroundJob],
            [
                HostFilesystemWrite,
                RuntimeStoreWrite,
                CoordinationRecord,
                ProcessSpawn,
                NetworkAccess,
            ],
            High,
            ServiceIdentity,
        ),
        // Daemon, auto-ingest, session lifecycle, and REST side effects.
        entry(
            "daemon.start",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "daemon.stop",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            Medium,
            ScopedGrantA2,
        ),
        entry(
            "daemon.tick",
            [Mcp, BackgroundJob],
            [GraphMutation, RuntimeStoreWrite, CoordinationRecord],
            High,
            ServiceIdentity,
        ),
        entry(
            "daemon.alerts_ack",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "auto_ingest.start_existing_roots",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "auto_ingest.change_roots",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "auto_ingest.stop",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            Medium,
            ScopedGrantA2,
        ),
        entry(
            "auto_ingest.tick",
            [Mcp, BackgroundJob],
            [GraphMutation, RuntimeStoreWrite, CoordinationRecord],
            High,
            ServiceIdentity,
        ),
        entry(
            "runtime.instance.save",
            [Rest],
            [RuntimeStoreWrite],
            High,
            ScopedGrantA2,
        ),
        entry(
            "runtime.target.save",
            [Rest],
            [RuntimeStoreWrite, NetworkAccess],
            High,
            ScopedGrantA2,
        ),
        entry(
            "runtime.delete_state.permanent",
            [Rest, Recovery],
            [RuntimeStoreWrite, HostFilesystemWrite, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "runtime.runnerd.announce",
            [Rest],
            [RuntimeStoreWrite, CoordinationRecord],
            Low,
            Ordinary,
        ),
        entry(
            "runtime.http.event_log_append",
            [Rest],
            [RuntimeStoreWrite, CoordinationRecord],
            Low,
            Ordinary,
        ),
        entry(
            "runtime.owner.boot",
            [Cli],
            [
                HostFilesystemWrite,
                RuntimeStoreWrite,
                GraphMutation,
                CoordinationRecord,
            ],
            High,
            ServiceIdentity,
        ),
        entry(
            "runtime.owner.shutdown",
            [Cli],
            [RuntimeStoreWrite, CoordinationRecord],
            Medium,
            ServiceIdentity,
        ),
        entry(
            "runtime.instance.heartbeat",
            [BackgroundJob],
            [RuntimeStoreWrite, CoordinationRecord],
            Low,
            ServiceIdentity,
        ),
        entry(
            "runtime.instance.gc",
            [BackgroundJob],
            [RuntimeStoreWrite, CoordinationRecord],
            Medium,
            ServiceIdentity,
        ),
        entry(
            "runtime.agent_memory.reload",
            [Cli, BackgroundJob],
            [GraphMutation, RuntimeStoreWrite],
            High,
            ServiceIdentity,
        ),
        entry(
            "runtime.presence.track_agent",
            [Mcp, Rest],
            [RuntimeStoreWrite, CoordinationRecord],
            Low,
            Ordinary,
        ),
        entry(
            "runtime.session.handshake",
            [Mcp, Rest],
            [RuntimeStoreWrite, CoordinationRecord],
            Low,
            Ordinary,
        ),
        entry(
            "runtime.root.self_heal",
            [Cli, BackgroundJob],
            [RuntimeStoreWrite, CoordinationRecord],
            Medium,
            ServiceIdentity,
        ),
        entry(
            "runtime.project_brain.evict_persist",
            [BackgroundJob],
            [RuntimeStoreWrite],
            Medium,
            ServiceIdentity,
        ),
        entry(
            "runtime.runner.secret_init",
            [Cli],
            [HostFilesystemWrite],
            High,
            ServiceIdentity,
        ),
        entry(
            "runtime.runner.heartbeat",
            [BackgroundJob],
            [NetworkAccess, CoordinationRecord],
            Low,
            ServiceIdentity,
        ),
        entry(
            "runtime.server.open_browser",
            [Cli],
            [ProcessSpawn],
            High,
            ScopedGrantA2,
        ),
        entry(
            "runtime.network_expose",
            [Cli],
            [NetworkExpose, SovereignMutation],
            Critical,
            PositiveSovereign,
        ),
        // Durable trails, perspectives, locks, calibration, and derived graph.
        entry(
            "trail.save",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "trail.resume",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "trail.merge",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            High,
            ScopedGrantA2,
        ),
        entry(
            "perspective.start",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "perspective.routes",
            [Mcp],
            [Read, RuntimeStoreWrite],
            Low,
            Ordinary,
        ),
        entry(
            "perspective.inspect",
            [Mcp],
            [Read, RuntimeStoreWrite],
            Low,
            Ordinary,
        ),
        entry(
            "perspective.peek",
            [Mcp],
            [Read, RuntimeStoreWrite],
            Low,
            Ordinary,
        ),
        entry(
            "perspective.follow",
            [Mcp],
            [Read, RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "perspective.suggest",
            [Mcp],
            [Read, RuntimeStoreWrite],
            Low,
            Ordinary,
        ),
        entry(
            "perspective.affinity",
            [Mcp],
            [Read, RuntimeStoreWrite],
            Low,
            Ordinary,
        ),
        entry(
            "perspective.branch",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "perspective.back",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "perspective.close",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "lock.create",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "lock.watch",
            [Mcp],
            [Read, RuntimeStoreWrite],
            Low,
            Ordinary,
        ),
        entry("lock.diff", [Mcp], [Read, RuntimeStoreWrite], Low, Ordinary),
        entry(
            "lock.rebase",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "lock.release",
            [Mcp],
            [RuntimeStoreWrite, CoordinationRecord],
            Medium,
            Ordinary,
        ),
        entry(
            "antibody.create",
            [Mcp],
            [RuntimeStoreWrite, GraphMutation],
            Medium,
            ScopedGrantA2,
        ),
        entry(
            "antibody.enable",
            [Mcp],
            [RuntimeStoreWrite, GraphMutation],
            Medium,
            ScopedGrantA2,
        ),
        entry(
            "antibody.disable",
            [Mcp],
            [RuntimeStoreWrite, GraphMutation],
            Medium,
            ScopedGrantA2,
        ),
        entry(
            "antibody.delete",
            [Mcp],
            [RuntimeStoreWrite, GraphMutation],
            Medium,
            ScopedGrantA2,
        ),
        entry(
            "calibration.predict",
            [Mcp],
            [RuntimeStoreWrite],
            Medium,
            ScopedGrantA2,
        ),
        entry(
            "calibration.envelope",
            [Mcp],
            [RuntimeStoreWrite],
            Medium,
            ScopedGrantA2,
        ),
        entry(
            "graph.ghost_edges",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite],
            High,
            ScopedGrantA2,
        ),
        entry(
            "graph.runtime_overlay",
            [Mcp],
            [GraphMutation, RuntimeStoreWrite],
            High,
            ScopedGrantA2,
        ),
        // Host CLI, hooks, release, recovery, and migration.
        entry(
            "cli.startup.install_bwrap_compat",
            [Cli],
            [HostFilesystemWrite],
            High,
            ScopedGrantA2,
        ),
        entry(
            "cli.inbox_sweep.distribute",
            [Cli],
            [HostFilesystemWrite],
            Medium,
            ScopedGrantA2,
        ),
        entry(
            "cli.init",
            [Cli],
            [HostFilesystemWrite],
            High,
            ScopedGrantA2,
        ),
        entry(
            "cli.install_skills",
            [Cli],
            [HostFilesystemWrite],
            High,
            ScopedGrantA2,
        ),
        entry(
            "cli.host.skills_apply",
            [Cli],
            [HostFilesystemWrite],
            High,
            ScopedGrantA2,
        ),
        entry(
            "cli.host.config_apply",
            [Cli],
            [HostFilesystemWrite],
            High,
            ScopedGrantA2,
        ),
        entry(
            "cli.host.doctrine_apply",
            [Cli],
            [HostFilesystemWrite],
            High,
            ScopedGrantA2,
        ),
        entry(
            "cli.host.hooks_apply",
            [Cli],
            [HostFilesystemWrite],
            High,
            ScopedGrantA2,
        ),
        entry(
            "cli.demo.execute",
            [Cli],
            [HostFilesystemWrite, ProcessSpawn, NetworkAccess],
            High,
            ScopedGrantA2,
        ),
        entry(
            "cli.smoke.execute",
            [Cli],
            [HostFilesystemWrite, ProcessSpawn, NetworkAccess],
            High,
            ScopedGrantA2,
        ),
        entry(
            "release.update.verify",
            [Cli],
            [HostFilesystemWrite, ProcessSpawn, NetworkAccess],
            High,
            ScopedGrantA2,
        ),
        entry(
            "cli.agent.trust",
            [Cli],
            [
                HostFilesystemWrite,
                RuntimeStoreWrite,
                GraphMutation,
                ProcessSpawn,
            ],
            High,
            ScopedGrantA2,
        ),
        entry(
            "cli.agent.orient",
            [Cli],
            [
                Read,
                HostFilesystemWrite,
                RuntimeStoreWrite,
                GraphMutation,
                ProcessSpawn,
            ],
            High,
            ScopedGrantA2,
        ),
        entry(
            "cli.agent.first_minute",
            [Cli],
            [
                HostFilesystemWrite,
                RuntimeStoreWrite,
                GraphMutation,
                ProcessSpawn,
                NetworkAccess,
            ],
            High,
            ScopedGrantA2,
        ),
        entry(
            "cli.agent.kickstart",
            [Cli],
            [
                HostFilesystemWrite,
                RuntimeStoreWrite,
                GraphMutation,
                ProcessSpawn,
                NetworkAccess,
            ],
            High,
            ScopedGrantA2,
        ),
        entry(
            "release.self_update.apply",
            [Cli],
            [
                HostFilesystemWrite,
                ExecutableReplacement,
                ProcessSpawn,
                ProcessSignal,
                NetworkAccess,
                SovereignMutation,
            ],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "release.self_update.restart",
            [Cli],
            [
                HostFilesystemWrite,
                ExecutableReplacement,
                ProcessSpawn,
                ProcessSignal,
                SovereignMutation,
            ],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "release.self_update.rollback",
            [Cli, Recovery],
            [
                HostFilesystemWrite,
                ExecutableReplacement,
                ProcessSignal,
                SovereignMutation,
            ],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "hook.session_start.first_minute",
            [Hook],
            [
                HostFilesystemWrite,
                RuntimeStoreWrite,
                GraphMutation,
                ProcessSpawn,
                NetworkAccess,
            ],
            High,
            ServiceIdentity,
        ),
        entry(
            "hook.task_start.first_minute",
            [Hook],
            [
                HostFilesystemWrite,
                RuntimeStoreWrite,
                GraphMutation,
                ProcessSpawn,
                NetworkAccess,
            ],
            High,
            ServiceIdentity,
        ),
        entry(
            "hook.agent_spawn.first_minute",
            [Hook],
            [
                HostFilesystemWrite,
                RuntimeStoreWrite,
                GraphMutation,
                ProcessSpawn,
                NetworkAccess,
            ],
            High,
            ServiceIdentity,
        ),
        entry(
            "hook.kickstart",
            [Hook],
            [
                HostFilesystemWrite,
                RuntimeStoreWrite,
                GraphMutation,
                ProcessSpawn,
                NetworkAccess,
            ],
            High,
            ServiceIdentity,
        ),
        entry(
            "runtime.isolated_agent.start",
            [Cli, Hook],
            [HostFilesystemWrite, ProcessSpawn],
            High,
            ServiceIdentity,
        ),
        entry(
            "runtime.isolated_agent.stop",
            [Cli, Hook],
            [HostFilesystemWrite, ProcessSignal],
            High,
            ServiceIdentity,
        ),
        entry(
            "migration.medulla.apply",
            [Migration],
            [
                HostFilesystemWrite,
                RuntimeStoreWrite,
                GraphMutation,
                SovereignMutation,
            ],
            Critical,
            PositiveSovereign,
        ),
        entry(
            "migration.medulla.rollback",
            [Migration, Recovery],
            [
                HostFilesystemWrite,
                RuntimeStoreWrite,
                GraphMutation,
                SovereignMutation,
            ],
            Critical,
            PositiveSovereign,
        ),
        // Immutable negative-only SafetyKernel catalog.
        entry(
            "safety.freeze_issuance",
            [BackgroundJob, Recovery],
            [FreezeIssuance],
            Critical,
            SafetyOnly,
        ),
        entry(
            "safety.epoch_fence",
            [BackgroundJob, Recovery],
            [EpochFence],
            Critical,
            SafetyOnly,
        ),
        entry(
            "safety.epoch_bump",
            [BackgroundJob, Recovery],
            [EpochBump],
            Critical,
            SafetyOnly,
        ),
        entry(
            "safety.revoke_capability",
            [BackgroundJob, Recovery],
            [RevokeCapability],
            Critical,
            SafetyOnly,
        ),
        entry(
            "safety.abort_prepared",
            [BackgroundJob, Recovery],
            [AbortPrepared],
            Critical,
            SafetyOnly,
        ),
        entry(
            "safety.demote_grant",
            [BackgroundJob, Recovery],
            [DemoteGrant],
            Critical,
            SafetyOnly,
        ),
        entry(
            "safety.rollback_signed_candidate",
            [BackgroundJob, Recovery],
            [RollbackSignedCandidate],
            Critical,
            SafetyOnly,
        ),
    ];

    entries.sort_by(|left, right| left.action.cmp(&right.action));
    let mut catalog = ActionCatalogV1 {
        schema: ACTION_CATALOG_SCHEMA.into(),
        catalog_version: M1ND10_ACTION_CATALOG_VERSION.into(),
        entries,
        catalog_digest: String::new(),
    };
    catalog.seal()?;
    catalog.validate()?;
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;

    fn action(value: &str) -> ActionId {
        ActionId::new(value).unwrap()
    }

    fn test_catalog(entry: ActionCatalogEntryV1) -> ActionCatalogV1 {
        let mut catalog = ActionCatalogV1 {
            schema: ACTION_CATALOG_SCHEMA.into(),
            catalog_version: "test-v1".into(),
            entries: vec![entry],
            catalog_digest: String::new(),
        };
        catalog.seal().unwrap();
        catalog
    }

    fn ordinary_entry(action_id: &str) -> ActionCatalogEntryV1 {
        entry(
            action_id,
            [Ingress::Mcp],
            [Effect::Read],
            RiskClass::Low,
            AuthorityFloor::Ordinary,
        )
    }

    #[test]
    fn brain_promotion_is_a_critical_positive_sovereign_action() {
        let catalog = m1nd10_action_catalog().expect("canonical catalog");
        let promote = catalog
            .entries
            .iter()
            .find(|entry| entry.action.as_str() == "brain.promote")
            .expect("brain.promote catalog entry");
        assert_eq!(promote.authority_floor, AuthorityFloor::PositiveSovereign);
        assert_eq!(promote.risk_class, RiskClass::Critical);
        assert!(promote
            .complete_effects
            .contains(&Effect::SovereignMutation));
    }

    #[test]
    fn wire_shape_is_exact_and_unknown_fields_are_denied() {
        let catalog = test_catalog(ordinary_entry("query.read"));
        let Value::Object(object) = serde_json::to_value(&catalog).unwrap() else {
            panic!("catalog must serialize as object");
        };
        assert_eq!(
            object.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            BTreeSet::from(["catalog_digest", "catalog_version", "entries", "schema"])
        );
        assert_eq!(
            object["entries"][0]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "action",
                "authority_floor",
                "complete_effects",
                "ingresses",
                "risk_class",
            ])
        );
        assert_eq!(
            serde_json::to_value(AuthorityFloor::ScopedGrantA2).unwrap(),
            json!("SCOPED_GRANT_A2")
        );
        assert_eq!(
            serde_json::to_value([
                Effect::HostFilesystemWrite,
                Effect::ExecutableReplacement,
                Effect::ProcessSignal,
                Effect::NetworkAccess,
            ])
            .unwrap(),
            json!([
                "HOST_FILESYSTEM_WRITE",
                "EXECUTABLE_REPLACEMENT",
                "PROCESS_SIGNAL",
                "NETWORK_ACCESS"
            ])
        );

        let mut top_level = serde_json::to_value(&catalog).unwrap();
        top_level
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(true));
        assert!(serde_json::from_value::<ActionCatalogV1>(top_level).is_err());

        let mut nested = serde_json::to_value(&catalog).unwrap();
        nested["entries"][0]
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(true));
        assert!(serde_json::from_value::<ActionCatalogV1>(nested).is_err());
    }

    #[test]
    fn self_hash_detects_any_catalog_change() {
        let mut catalog = m1nd10_action_catalog().unwrap();
        let validation = catalog.validate().unwrap();
        assert_eq!(validation.entry_count, validation.action_count);
        assert_eq!(validation.ingress_count, 7);
        assert_eq!(validation.computed_catalog_digest, catalog.catalog_digest);

        catalog.entries[0].risk_class = RiskClass::Low;
        assert!(matches!(
            catalog.validate(),
            Err(ActionCatalogError::CatalogDigestMismatch { .. })
                | Err(ActionCatalogError::PositiveSovereignMustBeCritical { .. })
        ));
    }

    #[test]
    fn duplicate_action_is_rejected() {
        let duplicate = ordinary_entry("query.read");
        let mut catalog = ActionCatalogV1 {
            schema: ACTION_CATALOG_SCHEMA.into(),
            catalog_version: "test-v1".into(),
            entries: vec![duplicate.clone(), duplicate],
            catalog_digest: String::new(),
        };
        catalog.seal().unwrap();
        assert!(matches!(
            catalog.validate(),
            Err(ActionCatalogError::DuplicateAction { .. })
        ));
    }

    #[test]
    fn empty_ingresses_and_effects_fail_closed() {
        let empty_ingresses = ActionCatalogEntryV1 {
            action: action("query.empty_ingress"),
            ingresses: BTreeSet::new(),
            complete_effects: BTreeSet::from([Effect::Read]),
            risk_class: RiskClass::Low,
            authority_floor: AuthorityFloor::Ordinary,
        };
        assert!(matches!(
            test_catalog(empty_ingresses).validate(),
            Err(ActionCatalogError::EmptyIngresses { .. })
        ));

        let empty_effects = ActionCatalogEntryV1 {
            action: action("query.empty_effects"),
            ingresses: BTreeSet::from([Ingress::Mcp]),
            complete_effects: BTreeSet::new(),
            risk_class: RiskClass::Low,
            authority_floor: AuthorityFloor::Ordinary,
        };
        assert!(matches!(
            test_catalog(empty_effects).validate(),
            Err(ActionCatalogError::EmptyEffects { .. })
        ));
    }

    #[test]
    fn sovereign_effect_and_floor_are_bijective_and_critical() {
        let sovereign_under_ordinary = ActionCatalogEntryV1 {
            complete_effects: BTreeSet::from([Effect::SovereignMutation]),
            ..ordinary_entry("governance.bad_ordinary")
        };
        assert!(matches!(
            test_catalog(sovereign_under_ordinary).validate(),
            Err(ActionCatalogError::SovereignEffectWithoutPositiveFloor { .. })
        ));

        let missing_effect = ActionCatalogEntryV1 {
            action: action("governance.missing_effect"),
            ingresses: BTreeSet::from([Ingress::Mcp]),
            complete_effects: BTreeSet::from([Effect::RuntimeStoreWrite]),
            risk_class: RiskClass::Critical,
            authority_floor: AuthorityFloor::PositiveSovereign,
        };
        assert!(matches!(
            test_catalog(missing_effect).validate(),
            Err(ActionCatalogError::PositiveFloorWithoutSovereignEffect { .. })
        ));

        let noncritical_sovereign = ActionCatalogEntryV1 {
            action: action("governance.noncritical"),
            ingresses: BTreeSet::from([Ingress::Mcp]),
            complete_effects: BTreeSet::from([Effect::SovereignMutation]),
            risk_class: RiskClass::High,
            authority_floor: AuthorityFloor::PositiveSovereign,
        };
        assert!(matches!(
            test_catalog(noncritical_sovereign).validate(),
            Err(ActionCatalogError::PositiveSovereignMustBeCritical { .. })
        ));
    }

    #[test]
    fn safety_only_is_negative_only_and_separate_from_positive_authority() {
        let safety = ActionCatalogEntryV1 {
            action: action("safety.test"),
            ingresses: BTreeSet::from([Ingress::BackgroundJob]),
            complete_effects: BTreeSet::from([Effect::FreezeIssuance, Effect::RevokeCapability]),
            risk_class: RiskClass::Critical,
            authority_floor: AuthorityFloor::SafetyOnly,
        };
        assert!(test_catalog(safety).validate().is_ok());

        let mixed = ActionCatalogEntryV1 {
            action: action("safety.mixed"),
            ingresses: BTreeSet::from([Ingress::BackgroundJob]),
            complete_effects: BTreeSet::from([Effect::FreezeIssuance, Effect::RuntimeStoreWrite]),
            risk_class: RiskClass::Critical,
            authority_floor: AuthorityFloor::SafetyOnly,
        };
        assert!(matches!(
            test_catalog(mixed).validate(),
            Err(ActionCatalogError::NonSafetyEffectInSafetyAction { .. })
        ));

        let leaked = ActionCatalogEntryV1 {
            action: action("governance.safety_leak"),
            ingresses: BTreeSet::from([Ingress::Mcp]),
            complete_effects: BTreeSet::from([Effect::SovereignMutation, Effect::FreezeIssuance]),
            risk_class: RiskClass::Critical,
            authority_floor: AuthorityFloor::PositiveSovereign,
        };
        assert!(matches!(
            test_catalog(leaked).validate(),
            Err(ActionCatalogError::SafetyEffectOutsideSafetyAction { .. })
        ));
    }

    #[test]
    fn executable_replacement_never_uses_ordinary_or_a2_authority() {
        let replacement = ActionCatalogEntryV1 {
            action: action("release.bad_replace"),
            ingresses: BTreeSet::from([Ingress::Cli]),
            complete_effects: BTreeSet::from([Effect::ExecutableReplacement]),
            risk_class: RiskClass::High,
            authority_floor: AuthorityFloor::ScopedGrantA2,
        };
        assert!(matches!(
            test_catalog(replacement).validate(),
            Err(ActionCatalogError::ExecutableReplacementWithoutPositiveFloor { .. })
        ));
    }

    #[test]
    fn all_audited_p0_actions_are_present_as_semantic_variants() {
        let catalog = m1nd10_action_catalog().unwrap();
        let actions: BTreeSet<&str> = catalog
            .entries
            .iter()
            .map(|entry| entry.action.as_str())
            .collect();
        let required = [
            "authority.authorize",
            "brain.bootstrap",
            "brain.promote",
            "graph.ingest.change_roots",
            "graph.ingest.replace",
            "graph.audit.replace",
            "graph.federate.replace",
            "graph.federate_auto.preview",
            "graph.federate_auto.execute",
            "store.persist.save_explicit_path",
            "store.persist.load_replace",
            "memory.memorize.explicit_path",
            "boot_memory.set",
            "boot_memory.delete",
            "source.apply.single",
            "source.apply.batch",
            "source.edit.preview",
            "source.edit.commit",
            "xray.apply.dry_run",
            "xray.apply.commit",
            "system_blocks.ratify",
            "system_blocks.receipt_import",
            "system_blocks.archive",
            "system_blocks.delete.permanent",
            "mission.post.landed",
            "mission.post.archive",
            "delegation.delegate",
            "daemon.stop",
            "daemon.tick",
            "auto_ingest.change_roots",
            "auto_ingest.stop",
            "auto_ingest.tick",
            "calibration.predict",
            "calibration.envelope",
            "graph.ghost_edges",
            "graph.runtime_overlay",
            "runtime.delete_state.permanent",
            "runtime.owner.boot",
            "runtime.session.handshake",
            "runtime.root.self_heal",
            "cli.startup.install_bwrap_compat",
            "cli.agent.trust",
            "cli.agent.orient",
            "cli.agent.first_minute",
            "cli.agent.kickstart",
            "hook.session_start.first_minute",
            "runner.mission.execute",
            "mission.curation_spawn",
            "release.self_update.apply",
            "release.self_update.restart",
            "release.self_update.rollback",
            "migration.medulla.apply",
            "migration.medulla.rollback",
        ];
        for required_action in required {
            assert!(
                actions.contains(required_action),
                "missing {required_action}"
            );
        }
    }

    #[test]
    fn audited_inventory_count_and_ingress_coverage_are_stable() {
        let catalog = m1nd10_action_catalog().unwrap();
        let ingresses: BTreeSet<Ingress> = catalog
            .entries
            .iter()
            .flat_map(|entry| entry.ingresses.iter().copied())
            .collect();
        // `graph.ingest.preview` is now an explicit read-only governed action
        // rather than an untracked pre-mutation side channel.  Keep this pin in
        // lock-step with the exhaustive consumer registry.
        assert_eq!(catalog.entries.len(), 173);
        assert_eq!(ingresses.len(), 7);
    }
}
