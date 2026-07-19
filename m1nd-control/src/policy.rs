use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{digest_canonical, CanonicalError};

pub const ACTION_POLICY_REGISTRY_SCHEMA: &str = "m1nd-action-policy-registry-v1";
pub const ACTION_POLICY_DIGEST_DOMAIN: &str = "m1nd-action-policy-registry-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Ingress {
    Mcp,
    Rest,
    Cli,
    Hook,
    BackgroundJob,
    Recovery,
    Migration,
}

/// Stable action identifier. Deserialization remains lossless; registries
/// reject an empty identifier during fail-closed validation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActionId(String);

impl ActionId {
    pub fn new(value: impl Into<String>) -> Result<Self, PolicyError> {
        let value = value.into();
        require_non_empty("action_id", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Canonical catalog form: two or more lowercase dotted segments; each
    /// segment may contain only ASCII lowercase letters, digits, or `_`.
    pub fn is_semantic_catalog_id(&self) -> bool {
        self.0.contains('.')
            && self.0.split('.').all(|segment| {
                !segment.is_empty()
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    })
            })
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActiveMode {
    HumanGated,
    PolicyAutonomous,
    FullAutonomy,
}

/// Authority path selected before policy evaluation.
///
/// `Ordinary` authenticates a client/session but creates no positive sovereign
/// authority. `Human`, `Policy`, and `AgentQuorum` are positive sovereign
/// variants. `SafetyKernel` is a separate negative-only path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorityVariant {
    Ordinary,
    Human,
    Policy,
    AgentQuorum,
    SafetyKernel,
}

impl AuthorityVariant {
    pub const fn is_positive_sovereign(self) -> bool {
        matches!(self, Self::Human | Self::Policy | Self::AgentQuorum)
    }

    pub const fn is_autonomous_positive(self) -> bool {
        matches!(self, Self::Policy | Self::AgentQuorum)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskClass {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AutonomyTier {
    A0Observe,
    A1Propose,
    A2Execute,
    A3AutonomousLand,
    A4AutonomousGovern,
    A5FullAutonomy,
}

/// Policy effects from PRD 6.4 plus the immutable negative safety allow-list
/// from PRD 6.16.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Effect {
    Read,
    GraphMutation,
    RuntimeStoreWrite,
    SourceFilesystemWrite,
    /// Writes outside the bound source tree and owner runtime store, including
    /// user configuration, hooks, compatibility shims, and temporary runtimes.
    HostFilesystemWrite,
    CoordinationRecord,
    MissionStateWrite,
    SovereignMutation,
    ProcessSpawn,
    /// Sends a signal to an existing process, including restart and rollback
    /// flows that terminate a served owner.
    ProcessSignal,
    /// Replaces an executable artifact that can become the next runtime owner.
    ExecutableReplacement,
    /// Permits outbound network activity. This is distinct from exposing a
    /// listening surface through `NetworkExpose`.
    NetworkAccess,
    NetworkExpose,
    FreezeIssuance,
    EpochFence,
    EpochBump,
    RevokeCapability,
    AbortPrepared,
    DemoteGrant,
    RollbackSignedCandidate,
}

impl Effect {
    pub const fn is_negative_safety(self) -> bool {
        matches!(
            self,
            Self::FreezeIssuance
                | Self::EpochFence
                | Self::EpochBump
                | Self::RevokeCapability
                | Self::AbortPrepared
                | Self::DemoteGrant
                | Self::RollbackSignedCandidate
        )
    }
}

/// One exact lookup key that can reach the owner-side policy middleware.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReachablePolicyTupleV1 {
    pub ingress: Ingress,
    pub action: ActionId,
    pub active_mode: ActiveMode,
    pub subject_id: String,
    pub authority_variant: AuthorityVariant,
    pub applicable_grant_id: Option<String>,
    pub applicable_tier: Option<AutonomyTier>,
    pub risk_class: RiskClass,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionPolicyRuleV1 {
    pub tuple: ReachablePolicyTupleV1,
    pub effects: BTreeSet<Effect>,
}

/// Minimum complete effect set for an action, independent of ingress or risk.
/// Every reachable rule for that action must contain this entire set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionEffectFloorV1 {
    pub action: ActionId,
    pub required_effects: BTreeSet<Effect>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionPolicyRegistryV1 {
    pub schema: String,
    pub policy_version: String,
    pub reachable_tuples: Vec<ReachablePolicyTupleV1>,
    pub rules: Vec<ActionPolicyRuleV1>,
    pub action_effect_floors: Vec<ActionEffectFloorV1>,
    pub policy_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyValidation {
    pub reachable_tuple_count: usize,
    pub rule_count: usize,
    pub action_count: usize,
    pub computed_policy_digest: String,
}

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("unsupported action policy registry schema '{actual}'")]
    Schema { actual: String },
    #[error("required field '{field}' is empty")]
    EmptyRequired { field: &'static str },
    #[error("action policy registry must declare at least one reachable tuple")]
    NoReachableTuples,
    #[error("action policy registry must declare at least one rule")]
    NoRules,
    #[error("action policy registry must declare at least one action effect floor")]
    NoEffectFloors,
    #[error("duplicate reachable policy tuple: {tuple:?}")]
    DuplicateReachableTuple { tuple: ReachablePolicyTupleV1 },
    #[error("duplicate action policy rule: {tuple:?}")]
    DuplicateRule { tuple: ReachablePolicyTupleV1 },
    #[error("duplicate effect floor for action '{action}'")]
    DuplicateEffectFloor { action: ActionId },
    #[error("rule for action '{action}' has no effects")]
    EmptyRuleEffects { action: ActionId },
    #[error("effect floor for action '{action}' is empty")]
    EmptyEffectFloor { action: ActionId },
    #[error("reachable tuple has no exact policy rule: {tuple:?}")]
    MissingRule { tuple: ReachablePolicyTupleV1 },
    #[error("policy rule is not declared reachable: {tuple:?}")]
    UnreachableRule { tuple: ReachablePolicyTupleV1 },
    #[error("reachable action '{action}' has no effect floor")]
    MissingEffectFloor { action: ActionId },
    #[error("effect floor names unreachable action '{action}'")]
    UnreachableEffectFloor { action: ActionId },
    #[error("rule for action '{action}' omits required effect {effect:?}")]
    MissingRequiredEffect { action: ActionId, effect: Effect },
    #[error("HUMAN authority forbids applicable_grant_id and applicable_tier")]
    HumanGrantOrTierForbidden,
    #[error("autonomous positive authority requires applicable_grant_id and applicable_tier")]
    AutonomousGrantAndTierRequired,
    #[error("SAFETY_KERNEL authority forbids applicable_grant_id and applicable_tier")]
    SafetyGrantOrTierForbidden,
    #[error("applicable_grant_id and applicable_tier must be present together")]
    IncompleteGrantTier,
    #[error("authority variant {authority_variant:?} is unreachable in mode {active_mode:?}")]
    AuthorityModeMismatch {
        active_mode: ActiveMode,
        authority_variant: AuthorityVariant,
    },
    #[error("SAFETY_KERNEL rule for action '{action}' contains non-safety effect {effect:?}")]
    NonSafetyEffectInSafetyRule { action: ActionId, effect: Effect },
    #[error("non-safety rule for action '{action}' contains safety effect {effect:?}")]
    SafetyEffectOutsideSafetyRule { action: ActionId, effect: Effect },
    #[error("ORDINARY rule for action '{action}' cannot contain SOVEREIGN_MUTATION")]
    SovereignEffectInOrdinaryRule { action: ActionId },
    #[error("positive sovereign rule for action '{action}' must contain SOVEREIGN_MUTATION")]
    MissingSovereignEffect { action: ActionId },
    #[error("policy digest mismatch: expected {expected}, observed {observed}")]
    PolicyDigestMismatch { expected: String, observed: String },
    #[error(transparent)]
    Canonical(#[from] CanonicalError),
}

impl ActionPolicyRegistryV1 {
    /// Compute the registry self-hash while omitting only `policy_digest`.
    pub fn compute_policy_digest(&self) -> Result<String, CanonicalError> {
        let mut value = serde_json::to_value(self)?;
        let object = value
            .as_object_mut()
            .expect("ActionPolicyRegistryV1 always serializes as an object");
        object.remove("policy_digest");
        digest_canonical(ACTION_POLICY_DIGEST_DOMAIN, &value)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.policy_digest = self.compute_policy_digest()?;
        Ok(())
    }

    /// Prove exact coverage and separation of ordinary, positive sovereign,
    /// and negative safety paths. This performs no authorization side effect.
    pub fn validate(&self) -> Result<PolicyValidation, PolicyError> {
        if self.schema != ACTION_POLICY_REGISTRY_SCHEMA {
            return Err(PolicyError::Schema {
                actual: self.schema.clone(),
            });
        }
        require_non_empty("policy_version", &self.policy_version)?;
        require_non_empty("policy_digest", &self.policy_digest)?;
        if self.reachable_tuples.is_empty() {
            return Err(PolicyError::NoReachableTuples);
        }
        if self.rules.is_empty() {
            return Err(PolicyError::NoRules);
        }
        if self.action_effect_floors.is_empty() {
            return Err(PolicyError::NoEffectFloors);
        }

        let mut reachable = BTreeSet::new();
        let mut reachable_actions = BTreeSet::new();
        for tuple in &self.reachable_tuples {
            validate_tuple(tuple)?;
            if !reachable.insert(tuple.clone()) {
                return Err(PolicyError::DuplicateReachableTuple {
                    tuple: tuple.clone(),
                });
            }
            reachable_actions.insert(tuple.action.clone());
        }

        let mut floors = BTreeMap::new();
        for floor in &self.action_effect_floors {
            require_non_empty("action_effect_floors[].action", floor.action.as_str())?;
            if floor.required_effects.is_empty() {
                return Err(PolicyError::EmptyEffectFloor {
                    action: floor.action.clone(),
                });
            }
            if floors
                .insert(floor.action.clone(), floor.required_effects.clone())
                .is_some()
            {
                return Err(PolicyError::DuplicateEffectFloor {
                    action: floor.action.clone(),
                });
            }
        }

        for action in &reachable_actions {
            if !floors.contains_key(action) {
                return Err(PolicyError::MissingEffectFloor {
                    action: action.clone(),
                });
            }
        }
        for action in floors.keys() {
            if !reachable_actions.contains(action) {
                return Err(PolicyError::UnreachableEffectFloor {
                    action: action.clone(),
                });
            }
        }

        let mut covered = BTreeSet::new();
        for rule in &self.rules {
            validate_tuple(&rule.tuple)?;
            if rule.effects.is_empty() {
                return Err(PolicyError::EmptyRuleEffects {
                    action: rule.tuple.action.clone(),
                });
            }
            if !covered.insert(rule.tuple.clone()) {
                return Err(PolicyError::DuplicateRule {
                    tuple: rule.tuple.clone(),
                });
            }
            if !reachable.contains(&rule.tuple) {
                return Err(PolicyError::UnreachableRule {
                    tuple: rule.tuple.clone(),
                });
            }

            validate_effect_path(rule)?;
            let required_effects = floors
                .get(&rule.tuple.action)
                .expect("reachable actions were checked for effect floors");
            if let Some(effect) = required_effects
                .iter()
                .find(|effect| !rule.effects.contains(effect))
            {
                return Err(PolicyError::MissingRequiredEffect {
                    action: rule.tuple.action.clone(),
                    effect: *effect,
                });
            }
        }

        if let Some(tuple) = reachable.iter().find(|tuple| !covered.contains(*tuple)) {
            return Err(PolicyError::MissingRule {
                tuple: tuple.clone(),
            });
        }

        let computed_policy_digest = self.compute_policy_digest()?;
        if self.policy_digest != computed_policy_digest {
            return Err(PolicyError::PolicyDigestMismatch {
                expected: computed_policy_digest,
                observed: self.policy_digest.clone(),
            });
        }

        Ok(PolicyValidation {
            reachable_tuple_count: reachable.len(),
            rule_count: covered.len(),
            action_count: reachable_actions.len(),
            computed_policy_digest,
        })
    }
}

fn validate_tuple(tuple: &ReachablePolicyTupleV1) -> Result<(), PolicyError> {
    require_non_empty("reachable_tuples[].action", tuple.action.as_str())?;
    require_non_empty("reachable_tuples[].subject_id", &tuple.subject_id)?;
    validate_optional_non_empty(
        "reachable_tuples[].applicable_grant_id",
        tuple.applicable_grant_id.as_deref(),
    )?;

    match tuple.authority_variant {
        AuthorityVariant::Human => {
            if tuple.applicable_grant_id.is_some() || tuple.applicable_tier.is_some() {
                return Err(PolicyError::HumanGrantOrTierForbidden);
            }
        }
        AuthorityVariant::Policy | AuthorityVariant::AgentQuorum => {
            if tuple.applicable_grant_id.is_none() || tuple.applicable_tier.is_none() {
                return Err(PolicyError::AutonomousGrantAndTierRequired);
            }
        }
        AuthorityVariant::SafetyKernel => {
            if tuple.applicable_grant_id.is_some() || tuple.applicable_tier.is_some() {
                return Err(PolicyError::SafetyGrantOrTierForbidden);
            }
        }
        AuthorityVariant::Ordinary => {
            if tuple.applicable_grant_id.is_some() != tuple.applicable_tier.is_some() {
                return Err(PolicyError::IncompleteGrantTier);
            }
        }
    }

    let mode_matches = match tuple.active_mode {
        ActiveMode::HumanGated => matches!(
            tuple.authority_variant,
            AuthorityVariant::Ordinary | AuthorityVariant::Human | AuthorityVariant::SafetyKernel
        ),
        ActiveMode::PolicyAutonomous => matches!(
            tuple.authority_variant,
            AuthorityVariant::Ordinary
                | AuthorityVariant::Human
                | AuthorityVariant::Policy
                | AuthorityVariant::SafetyKernel
        ),
        ActiveMode::FullAutonomy => matches!(
            tuple.authority_variant,
            AuthorityVariant::Ordinary
                | AuthorityVariant::AgentQuorum
                | AuthorityVariant::SafetyKernel
        ),
    };
    if !mode_matches {
        return Err(PolicyError::AuthorityModeMismatch {
            active_mode: tuple.active_mode,
            authority_variant: tuple.authority_variant,
        });
    }

    Ok(())
}

fn validate_effect_path(rule: &ActionPolicyRuleV1) -> Result<(), PolicyError> {
    let action = &rule.tuple.action;
    match rule.tuple.authority_variant {
        AuthorityVariant::SafetyKernel => {
            if let Some(effect) = rule
                .effects
                .iter()
                .find(|effect| !effect.is_negative_safety())
            {
                return Err(PolicyError::NonSafetyEffectInSafetyRule {
                    action: action.clone(),
                    effect: *effect,
                });
            }
        }
        authority_variant => {
            if let Some(effect) = rule
                .effects
                .iter()
                .find(|effect| effect.is_negative_safety())
            {
                return Err(PolicyError::SafetyEffectOutsideSafetyRule {
                    action: action.clone(),
                    effect: *effect,
                });
            }
            if authority_variant == AuthorityVariant::Ordinary
                && rule.effects.contains(&Effect::SovereignMutation)
            {
                return Err(PolicyError::SovereignEffectInOrdinaryRule {
                    action: action.clone(),
                });
            }
            if authority_variant.is_positive_sovereign()
                && !rule.effects.contains(&Effect::SovereignMutation)
            {
                return Err(PolicyError::MissingSovereignEffect {
                    action: action.clone(),
                });
            }
        }
    }
    Ok(())
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), PolicyError> {
    if value.trim().is_empty() {
        return Err(PolicyError::EmptyRequired { field });
    }
    Ok(())
}

fn validate_optional_non_empty(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), PolicyError> {
    if value.is_some_and(|value| value.trim().is_empty()) {
        return Err(PolicyError::EmptyRequired { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;

    fn action(value: &str) -> ActionId {
        ActionId::new(value).unwrap()
    }

    fn ordinary_tuple(action_id: &str) -> ReachablePolicyTupleV1 {
        ReachablePolicyTupleV1 {
            ingress: Ingress::Mcp,
            action: action(action_id),
            active_mode: ActiveMode::HumanGated,
            subject_id: "client:codex".into(),
            authority_variant: AuthorityVariant::Ordinary,
            applicable_grant_id: None,
            applicable_tier: None,
            risk_class: RiskClass::Low,
        }
    }

    fn registry_for(
        tuple: ReachablePolicyTupleV1,
        effects: BTreeSet<Effect>,
    ) -> ActionPolicyRegistryV1 {
        let mut registry = ActionPolicyRegistryV1 {
            schema: ACTION_POLICY_REGISTRY_SCHEMA.into(),
            policy_version: "2026-07-18.1".into(),
            reachable_tuples: vec![tuple.clone()],
            rules: vec![ActionPolicyRuleV1 {
                tuple: tuple.clone(),
                effects: effects.clone(),
            }],
            action_effect_floors: vec![ActionEffectFloorV1 {
                action: tuple.action,
                required_effects: effects,
            }],
            policy_digest: String::new(),
        };
        registry.seal().unwrap();
        registry
    }

    fn set<const N: usize>(effects: [Effect; N]) -> BTreeSet<Effect> {
        effects.into_iter().collect()
    }

    #[test]
    fn all_ingresses_and_typed_enums_have_exact_wire_names() {
        assert_eq!(
            serde_json::to_value([
                Ingress::Mcp,
                Ingress::Rest,
                Ingress::Cli,
                Ingress::Hook,
                Ingress::BackgroundJob,
                Ingress::Recovery,
                Ingress::Migration,
            ])
            .unwrap(),
            json!([
                "MCP",
                "REST",
                "CLI",
                "HOOK",
                "BACKGROUND_JOB",
                "RECOVERY",
                "MIGRATION"
            ])
        );
        assert_eq!(
            serde_json::to_value(AutonomyTier::A5FullAutonomy).unwrap(),
            json!("A5_FULL_AUTONOMY")
        );
        assert_eq!(
            serde_json::to_value(AuthorityVariant::SafetyKernel).unwrap(),
            json!("SAFETY_KERNEL")
        );
    }

    #[test]
    fn registry_has_exact_wire_shape_and_denies_unknown_fields() {
        let registry = registry_for(ordinary_tuple("seek"), set([Effect::Read]));
        let Value::Object(object) = serde_json::to_value(&registry).unwrap() else {
            panic!("registry must serialize as an object");
        };
        assert_eq!(
            object.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "action_effect_floors",
                "policy_digest",
                "policy_version",
                "reachable_tuples",
                "rules",
                "schema",
            ])
        );
        let tuple_object = object["reachable_tuples"][0].as_object().unwrap();
        assert_eq!(
            tuple_object
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "action",
                "active_mode",
                "applicable_grant_id",
                "applicable_tier",
                "authority_variant",
                "ingress",
                "risk_class",
                "subject_id",
            ])
        );
        assert_eq!(
            object["rules"][0]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["effects", "tuple"])
        );
        assert_eq!(
            object["action_effect_floors"][0]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["action", "required_effects"])
        );

        let mut value = serde_json::to_value(&registry).unwrap();
        value["unexpected"] = json!(true);
        assert!(serde_json::from_value::<ActionPolicyRegistryV1>(value).is_err());
    }

    #[test]
    fn complete_multi_effect_policy_validates() {
        let tuple = ordinary_tuple("debrief");
        let effects = set([
            Effect::CoordinationRecord,
            Effect::RuntimeStoreWrite,
            Effect::GraphMutation,
        ]);
        let registry = registry_for(tuple, effects);
        let validation = registry.validate().unwrap();
        assert_eq!(validation.reachable_tuple_count, 1);
        assert_eq!(validation.rule_count, 1);
        assert_eq!(validation.action_count, 1);
        assert_eq!(validation.computed_policy_digest, registry.policy_digest);
    }

    #[test]
    fn canonical_self_hash_omits_only_digest_and_binds_policy_bytes() {
        let mut registry = registry_for(
            ordinary_tuple("memorize"),
            set([Effect::RuntimeStoreWrite, Effect::GraphMutation]),
        );
        let expected = registry.compute_policy_digest().unwrap();
        registry.policy_digest = "placeholder-does-not-enter-self-hash".into();
        assert_eq!(registry.compute_policy_digest().unwrap(), expected);

        registry.rules[0].effects.insert(Effect::Read);
        assert_ne!(registry.compute_policy_digest().unwrap(), expected);
        assert!(matches!(
            registry.validate(),
            Err(PolicyError::MissingRequiredEffect { .. })
                | Err(PolicyError::PolicyDigestMismatch { .. })
        ));
    }

    #[test]
    fn missing_tuple_duplicate_rule_and_unreachable_rule_fail_closed() {
        let tuple = ordinary_tuple("seek");
        let mut missing = registry_for(tuple.clone(), set([Effect::Read]));
        missing.rules.clear();
        assert!(matches!(missing.validate(), Err(PolicyError::NoRules)));

        let mut duplicate = registry_for(tuple.clone(), set([Effect::Read]));
        duplicate.rules.push(duplicate.rules[0].clone());
        duplicate.seal().unwrap();
        assert!(matches!(
            duplicate.validate(),
            Err(PolicyError::DuplicateRule { .. })
        ));

        let mut unreachable = registry_for(tuple, set([Effect::Read]));
        let mut extra_rule = unreachable.rules[0].clone();
        extra_rule.tuple.ingress = Ingress::Rest;
        unreachable.rules.push(extra_rule);
        unreachable.seal().unwrap();
        assert!(matches!(
            unreachable.validate(),
            Err(PolicyError::UnreachableRule { .. })
        ));
    }

    #[test]
    fn exact_coverage_rejects_one_missing_tuple_duplicate_reachability_and_missing_floor() {
        let first = ordinary_tuple("seek");
        let mut second = first.clone();
        second.ingress = Ingress::Rest;

        let mut missing = registry_for(first.clone(), set([Effect::Read]));
        missing.reachable_tuples.push(second);
        missing.seal().unwrap();
        assert!(matches!(
            missing.validate(),
            Err(PolicyError::MissingRule { .. })
        ));

        let mut duplicate_reachable = registry_for(first, set([Effect::Read]));
        duplicate_reachable
            .reachable_tuples
            .push(duplicate_reachable.reachable_tuples[0].clone());
        duplicate_reachable.seal().unwrap();
        assert!(matches!(
            duplicate_reachable.validate(),
            Err(PolicyError::DuplicateReachableTuple { .. })
        ));

        let mut missing_floor = registry_for(ordinary_tuple("seek"), set([Effect::Read]));
        missing_floor.action_effect_floors.clear();
        missing_floor.seal().unwrap();
        assert!(matches!(
            missing_floor.validate(),
            Err(PolicyError::NoEffectFloors)
        ));
    }

    #[test]
    fn omitted_required_multi_effect_fails() {
        let tuple = ordinary_tuple("debrief");
        let required = set([
            Effect::CoordinationRecord,
            Effect::RuntimeStoreWrite,
            Effect::GraphMutation,
        ]);
        let mut registry = registry_for(tuple, required);
        registry.rules[0].effects.remove(&Effect::GraphMutation);
        registry.seal().unwrap();
        assert!(matches!(
            registry.validate(),
            Err(PolicyError::MissingRequiredEffect {
                effect: Effect::GraphMutation,
                ..
            })
        ));
    }

    #[test]
    fn human_forbids_grant_and_autonomous_variants_require_grant_and_tier() {
        let mut human = ordinary_tuple("land");
        human.authority_variant = AuthorityVariant::Human;
        human.applicable_grant_id = Some("grant:wrong".into());
        human.applicable_tier = Some(AutonomyTier::A3AutonomousLand);
        let human_registry = registry_for(human, set([Effect::SovereignMutation]));
        assert!(matches!(
            human_registry.validate(),
            Err(PolicyError::HumanGrantOrTierForbidden)
        ));

        let autonomous = ReachablePolicyTupleV1 {
            ingress: Ingress::BackgroundJob,
            action: action("land"),
            active_mode: ActiveMode::PolicyAutonomous,
            subject_id: "agent:lander".into(),
            authority_variant: AuthorityVariant::Policy,
            applicable_grant_id: Some("grant:land".into()),
            applicable_tier: None,
            risk_class: RiskClass::Low,
        };
        let autonomous_registry = registry_for(autonomous, set([Effect::SovereignMutation]));
        assert!(matches!(
            autonomous_registry.validate(),
            Err(PolicyError::AutonomousGrantAndTierRequired)
        ));
    }

    #[test]
    fn active_mode_selects_one_positive_sovereign_provider_family() {
        let cases = [
            (ActiveMode::HumanGated, AuthorityVariant::Policy, false),
            (ActiveMode::HumanGated, AuthorityVariant::AgentQuorum, false),
            (ActiveMode::PolicyAutonomous, AuthorityVariant::Human, true),
            (ActiveMode::PolicyAutonomous, AuthorityVariant::Policy, true),
            (
                ActiveMode::PolicyAutonomous,
                AuthorityVariant::AgentQuorum,
                false,
            ),
            (ActiveMode::FullAutonomy, AuthorityVariant::Human, false),
            (ActiveMode::FullAutonomy, AuthorityVariant::Policy, false),
            (
                ActiveMode::FullAutonomy,
                AuthorityVariant::AgentQuorum,
                true,
            ),
        ];

        for (mode, variant, allowed) in cases {
            let tuple = ReachablePolicyTupleV1 {
                ingress: Ingress::BackgroundJob,
                action: action("governance.ratify"),
                active_mode: mode,
                subject_id: "authority:test".into(),
                authority_variant: variant,
                applicable_grant_id: variant
                    .is_autonomous_positive()
                    .then(|| "grant:test".into()),
                applicable_tier: variant
                    .is_autonomous_positive()
                    .then_some(AutonomyTier::A4AutonomousGovern),
                risk_class: RiskClass::Critical,
            };
            let result = registry_for(tuple, set([Effect::SovereignMutation])).validate();
            assert_eq!(
                result.is_ok(),
                allowed,
                "mode {mode:?} with authority {variant:?}"
            );
            if !allowed {
                assert!(matches!(
                    result,
                    Err(PolicyError::AuthorityModeMismatch { .. })
                ));
            }
        }
    }

    #[test]
    fn ordinary_positive_and_safety_paths_cannot_be_confused() {
        let ordinary = registry_for(ordinary_tuple("ratify"), set([Effect::SovereignMutation]));
        assert!(matches!(
            ordinary.validate(),
            Err(PolicyError::SovereignEffectInOrdinaryRule { .. })
        ));

        let mut positive_tuple = ordinary_tuple("land");
        positive_tuple.authority_variant = AuthorityVariant::Human;
        let positive = registry_for(positive_tuple, set([Effect::RuntimeStoreWrite]));
        assert!(matches!(
            positive.validate(),
            Err(PolicyError::MissingSovereignEffect { .. })
        ));

        let safety_tuple = ReachablePolicyTupleV1 {
            ingress: Ingress::Recovery,
            action: action("safety_rollback"),
            active_mode: ActiveMode::FullAutonomy,
            subject_id: "actuator:safety".into(),
            authority_variant: AuthorityVariant::SafetyKernel,
            applicable_grant_id: None,
            applicable_tier: None,
            risk_class: RiskClass::Critical,
        };
        let safety = registry_for(
            safety_tuple.clone(),
            set([Effect::FreezeIssuance, Effect::RollbackSignedCandidate]),
        );
        assert!(safety.validate().is_ok());

        let unsafe_positive = registry_for(safety_tuple, set([Effect::SovereignMutation]));
        assert!(matches!(
            unsafe_positive.validate(),
            Err(PolicyError::NonSafetyEffectInSafetyRule { .. })
        ));
    }

    #[test]
    fn blank_action_and_unknown_enum_variant_are_rejected() {
        assert!(ActionId::new("   ").is_err());
        assert!(serde_json::from_value::<Ingress>(json!("SOCKET")).is_err());

        let mut value = serde_json::to_value(ordinary_tuple("seek")).unwrap();
        value["extra_dimension"] = json!("bypass");
        assert!(serde_json::from_value::<ReachablePolicyTupleV1>(value).is_err());
    }
}
