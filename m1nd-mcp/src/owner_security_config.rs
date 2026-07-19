//! Fail-closed owner trust configuration for the G2/G3 production assembly.
//!
//! This file contains public trust anchors and relative durable roots only. It
//! never contains a private key, signer secret, bearer token, or software-test
//! fallback. A separate protected root pins `(config_epoch, config_digest)` so
//! replacing the JSON file with an older, otherwise valid copy is detected.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use m1nd_control::Role;
use m1nd_control::{digest_canonical, ActionPolicyRegistryV1, VerificationKeyRegistryV1};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::authority_runtime::{
    AuthorityRuntime, AuthorityRuntimeError, ProtectedEpochAssurance, ProtectedEpochBackend,
};
use crate::authority_runtime::{AuthorityRuntimeConfig, PinnedServiceIdentityV1};
use crate::authority_transport::{
    owner_authority_components_with_external_mutation, OwnerAuthorityComponentInputsV1,
    OwnerAuthorityServiceV1,
};
use crate::authority_wal::{AuthorityWalCryptoAssurance, AuthorityWalRecordCrypto};
use crate::autonomy_manifest::AutonomyAdmissionOwner;
use crate::mission_service::MissionServiceConfigV1;
use crate::mission_service_transport::{
    MissionServiceTransportError, MissionServiceTransportFacade,
    OwnerBrokerMissionServiceAuthorityProviderV1,
};
use crate::owner_authorization_broker::OwnerAuthorityLinearizationV1;
use crate::owner_authorization_broker::OwnerAuthorizationBrokerConfigV1;
use crate::protected_journal_head::{
    ProtectedJournalHeadAssuranceV1, SharedProtectedJournalHeadBackendV1,
};
use m1nd_control::autonomy_runtime::AutonomyRuntimeAssurance;

pub const OWNER_SECURITY_CONFIG_SCHEMA: &str = "m1nd-owner-security-config-v1";
pub const OWNER_SECURITY_CONFIG_DIGEST_DOMAIN: &str = "m1nd-owner-security-config-v1";
pub const MAX_OWNER_SECURITY_CONFIG_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OwnerSecurityConfigRootAssuranceV1 {
    SoftwareTestOnlyNotProven,
    HardwareProtectedAttested,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSecurityConfigRootV1 {
    pub config_epoch: u64,
    pub config_digest: String,
}

/// This backend is deliberately distinct from AuthorityRuntime's protected
/// state backend. Reusing one monotonic slot for two domains would let config
/// and runtime epochs overwrite one another.
pub trait ProtectedOwnerSecurityConfigRootBackendV1: Send {
    fn assurance(&self) -> OwnerSecurityConfigRootAssuranceV1;
    fn read_latest(&self) -> Result<Option<OwnerSecurityConfigRootV1>, String>;
    fn compare_and_advance(
        &mut self,
        expected: Option<&OwnerSecurityConfigRootV1>,
        next: &OwnerSecurityConfigRootV1,
    ) -> Result<(), String>;
}

/// Explicit development/test root. Production loading rejects this assurance.
#[derive(Clone, Default)]
pub struct SoftwareTestOwnerSecurityConfigRootBackendV1 {
    state: Arc<Mutex<Option<OwnerSecurityConfigRootV1>>>,
}

impl SoftwareTestOwnerSecurityConfigRootBackendV1 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Option<OwnerSecurityConfigRootV1> {
        self.state.lock().clone()
    }
}

impl ProtectedOwnerSecurityConfigRootBackendV1 for SoftwareTestOwnerSecurityConfigRootBackendV1 {
    fn assurance(&self) -> OwnerSecurityConfigRootAssuranceV1 {
        OwnerSecurityConfigRootAssuranceV1::SoftwareTestOnlyNotProven
    }

    fn read_latest(&self) -> Result<Option<OwnerSecurityConfigRootV1>, String> {
        Ok(self.state.lock().clone())
    }

    fn compare_and_advance(
        &mut self,
        expected: Option<&OwnerSecurityConfigRootV1>,
        next: &OwnerSecurityConfigRootV1,
    ) -> Result<(), String> {
        let mut current = self.state.lock();
        if current.as_ref() != expected {
            return Err("owner security config root compare-and-swap mismatch".to_string());
        }
        if next.config_epoch != expected.map_or(1, |root| root.config_epoch.saturating_add(1)) {
            return Err("owner security config root epoch must advance exactly once".to_string());
        }
        *current = Some(next.clone());
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSecurityConfigV1 {
    pub schema: String,
    pub config_epoch: u64,
    pub previous_config_digest: Option<String>,
    /// Roots are relative to the directory containing this file. Absolute,
    /// parent-traversing, and platform-prefixed paths are refused.
    pub authority_runtime_root: String,
    pub authorization_broker_root: String,
    pub mission_service_root: String,
    pub organism_id: String,
    pub repo_id: String,
    pub brain_id: String,
    pub audience: String,
    pub constitution_digest: String,
    pub constitution_epoch: u64,
    pub grants_digest: String,
    pub policy_registry_digest: String,
    pub policy_registry: ActionPolicyRegistryV1,
    pub service_identities: BTreeMap<String, PinnedServiceIdentityV1>,
    pub safety_kernel_digest: String,
    pub safety_actuator_identity_key_binary_policy_digest: String,
    pub verification_keys: VerificationKeyRegistryV1,
    /// Owner-pinned operational role for every subject represented in
    /// `verification_keys`. Wire requests may repeat this value only as a
    /// binding assertion; they can never select or elevate it.
    pub session_roles: BTreeMap<String, Role>,
    pub max_future_clock_skew_ms: u64,
    pub authorization_reservation_ttl_ms: u64,
    pub authorization_terminal_retention_ms: u64,
    pub config_digest: String,
}

impl OwnerSecurityConfigV1 {
    pub fn compute_config_digest(&self) -> Result<String, OwnerSecurityConfigError> {
        let mut value = serde_json::to_value(self)?;
        value
            .as_object_mut()
            .expect("OwnerSecurityConfigV1 serializes as an object")
            .remove("config_digest");
        Ok(digest_canonical(
            OWNER_SECURITY_CONFIG_DIGEST_DOMAIN,
            &value,
        )?)
    }

    pub fn seal(&mut self) -> Result<(), OwnerSecurityConfigError> {
        self.config_digest = self.compute_config_digest()?;
        Ok(())
    }

    pub fn validate(&self, owner_now_ms: u64) -> Result<(), OwnerSecurityConfigError> {
        if self.schema != OWNER_SECURITY_CONFIG_SCHEMA {
            return Err(OwnerSecurityConfigError::Invalid {
                code: "owner_security_config_schema_mismatch",
                detail: self.schema.clone(),
            });
        }
        if self.config_epoch == 0
            || (self.config_epoch == 1 && self.previous_config_digest.is_some())
            || (self.config_epoch > 1
                && !self
                    .previous_config_digest
                    .as_deref()
                    .is_some_and(is_digest))
        {
            return Err(OwnerSecurityConfigError::Invalid {
                code: "owner_security_config_epoch_chain_invalid",
                detail: "epoch one has no predecessor; later epochs require one digest".to_string(),
            });
        }
        for (field, value) in [
            ("organism_id", self.organism_id.as_str()),
            ("repo_id", self.repo_id.as_str()),
            ("brain_id", self.brain_id.as_str()),
            ("audience", self.audience.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(OwnerSecurityConfigError::Invalid {
                    code: "owner_security_config_required_field_empty",
                    detail: field.to_string(),
                });
            }
        }
        for (field, digest) in [
            ("constitution_digest", self.constitution_digest.as_str()),
            ("grants_digest", self.grants_digest.as_str()),
            (
                "policy_registry_digest",
                self.policy_registry_digest.as_str(),
            ),
            ("safety_kernel_digest", self.safety_kernel_digest.as_str()),
            (
                "safety_actuator_identity_key_binary_policy_digest",
                self.safety_actuator_identity_key_binary_policy_digest
                    .as_str(),
            ),
            ("config_digest", self.config_digest.as_str()),
        ] {
            if !is_digest(digest) {
                return Err(OwnerSecurityConfigError::Invalid {
                    code: "owner_security_config_digest_invalid",
                    detail: field.to_string(),
                });
            }
        }
        let authority_runtime_root = normalized_relative_root(&self.authority_runtime_root)?;
        let authorization_broker_root = normalized_relative_root(&self.authorization_broker_root)?;
        let mission_service_root = normalized_relative_root(&self.mission_service_root)?;
        if roots_overlap(&authority_runtime_root, &authorization_broker_root)
            || roots_overlap(&authority_runtime_root, &mission_service_root)
            || roots_overlap(&authorization_broker_root, &mission_service_root)
        {
            return Err(OwnerSecurityConfigError::Invalid {
                code: "owner_security_config_roots_overlap",
                detail: "authority, broker, and mission roots must be disjoint (nested roots are forbidden)"
                    .to_string(),
            });
        }
        self.policy_registry
            .validate()
            .map_err(|error| OwnerSecurityConfigError::Invalid {
                code: "owner_security_policy_invalid",
                detail: error.to_string(),
            })?;
        if self.policy_registry.policy_digest != self.policy_registry_digest {
            return Err(OwnerSecurityConfigError::Invalid {
                code: "owner_security_policy_digest_mismatch",
                detail: "embedded policy digest differs from the pinned digest".to_string(),
            });
        }
        self.verification_keys
            .validate(owner_now_ms, self.max_future_clock_skew_ms)
            .map_err(|error| OwnerSecurityConfigError::Invalid {
                code: "owner_security_verification_keys_invalid",
                detail: error.to_string(),
            })?;
        let verification_subjects = self
            .verification_keys
            .keys
            .values()
            .map(|key| key.subject_id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let configured_subjects = self
            .session_roles
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        if verification_subjects != configured_subjects
            || self.session_roles.iter().any(|(subject_id, role)| {
                subject_id.trim().is_empty() || *role == Role::MissionService
            })
        {
            return Err(OwnerSecurityConfigError::Invalid {
                code: "owner_security_session_roles_invalid",
                detail: "session_roles must pin exactly one non-service role for every verification-key subject"
                    .to_string(),
            });
        }
        if self.authorization_reservation_ttl_ms == 0
            || self.authorization_terminal_retention_ms == 0
        {
            return Err(OwnerSecurityConfigError::Invalid {
                code: "owner_security_config_retention_invalid",
                detail: "lease reservation and terminal retention must be non-zero".to_string(),
            });
        }
        if self.compute_config_digest()? != self.config_digest {
            return Err(OwnerSecurityConfigError::Invalid {
                code: "owner_security_config_digest_mismatch",
                detail: "config body differs from its canonical self-digest".to_string(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct LoadedOwnerSecurityConfigV1 {
    source_path: PathBuf,
    config: OwnerSecurityConfigV1,
    root_assurance: OwnerSecurityConfigRootAssuranceV1,
    authority_runtime_root: PathBuf,
    authorization_broker_root: PathBuf,
    mission_service_root: PathBuf,
}

impl LoadedOwnerSecurityConfigV1 {
    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    pub fn config_digest(&self) -> &str {
        &self.config.config_digest
    }

    pub fn config_epoch(&self) -> u64 {
        self.config.config_epoch
    }

    pub fn root_assurance(&self) -> OwnerSecurityConfigRootAssuranceV1 {
        self.root_assurance
    }

    pub fn authority_runtime_config(&self) -> AuthorityRuntimeConfig {
        AuthorityRuntimeConfig {
            root: self.authority_runtime_root.clone(),
            organism_id: self.config.organism_id.clone(),
            repo_id: self.config.repo_id.clone(),
            brain_id: self.config.brain_id.clone(),
            audience: self.config.audience.clone(),
            constitution_digest: self.config.constitution_digest.clone(),
            constitution_epoch: self.config.constitution_epoch,
            grants_digest: self.config.grants_digest.clone(),
            policy_registry_digest: self.config.policy_registry_digest.clone(),
            policy_registry: self.config.policy_registry.clone(),
            service_identities: self.config.service_identities.clone(),
            safety_kernel_digest: self.config.safety_kernel_digest.clone(),
            safety_actuator_identity_key_binary_policy_digest: self
                .config
                .safety_actuator_identity_key_binary_policy_digest
                .clone(),
            max_future_clock_skew_ms: self.config.max_future_clock_skew_ms,
        }
    }

    pub fn authorization_broker_config(&self) -> OwnerAuthorizationBrokerConfigV1 {
        OwnerAuthorizationBrokerConfigV1 {
            root: self.authorization_broker_root.clone(),
            reservation_ttl_ms: self.config.authorization_reservation_ttl_ms,
            minimum_terminal_retention_ms: self.config.authorization_terminal_retention_ms,
        }
    }

    pub fn verification_keys(&self) -> Arc<VerificationKeyRegistryV1> {
        Arc::new(self.config.verification_keys.clone())
    }

    pub fn session_roles(&self) -> Arc<BTreeMap<String, Role>> {
        Arc::new(self.config.session_roles.clone())
    }

    pub fn max_future_clock_skew_ms(&self) -> u64 {
        self.config.max_future_clock_skew_ms
    }

    pub fn mission_service_root(&self) -> &Path {
        &self.mission_service_root
    }

    fn revalidate_durable_roots(&self) -> Result<(), OwnerSecurityConfigError> {
        let base = self
            .source_path
            .parent()
            .ok_or_else(|| OwnerSecurityConfigError::Invalid {
                code: "owner_security_config_path_invalid",
                detail: self.source_path.display().to_string(),
            })?;
        for root in [
            &self.authority_runtime_root,
            &self.authorization_broker_root,
            &self.mission_service_root,
        ] {
            refuse_symlink_components_beneath(base, root)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerAuthorityStartupV1 {
    /// First installation only. The runtime is created HUMAN_GATED, FROZEN,
    /// and cannot issue positive authority until its signed bootstrap ceremony.
    BootstrapFrozen,
    /// Normal restart. Missing/corrupt/rolled-back runtime state is refused.
    OpenExisting,
}

pub struct OwnerAuthorityAssemblyV1 {
    loaded_security: LoadedOwnerSecurityConfigV1,
    authority_runtime: Arc<AuthorityRuntime>,
    authority_service: Arc<OwnerAuthorityServiceV1>,
    mission_authority_provider: Arc<OwnerBrokerMissionServiceAuthorityProviderV1>,
    mission_service: Arc<MissionServiceTransportFacade>,
    external_mutation_service: Arc<crate::external_mutation_service::ExternalMutationServiceV1>,
    autonomy_owner: Option<Arc<dyn AutonomyAdmissionOwner>>,
}

/// HTTP boot policy for the production G2/G3 pair. `Required` is intended for
/// owner deployments that advertise the authority surface: absence of a fully
/// preflighted production assembly is then a boot error, never a silent 503
/// downgrade. `OptionalNotInstalled` keeps legacy/read-only deployments honest
/// without manufacturing software assurance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerAuthorityBootRequirementV1 {
    OptionalNotInstalled,
    Required,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerAuthorityInstallationStatusV1 {
    Installed,
    NotInstalled,
}

impl OwnerAuthorityAssemblyV1 {
    pub fn authority_status(
        &self,
    ) -> Result<crate::authority_runtime::AuthorityRuntimeStatusV1, AuthorityRuntimeError> {
        self.authority_runtime.status()
    }

    pub fn security_config_digest(&self) -> &str {
        self.loaded_security.config_digest()
    }
}

#[cfg(feature = "serve")]
impl OwnerAuthorityAssemblyV1 {
    /// Explicit final boot seam. Callers build the ordinary AppState first and
    /// then install both halves together; installing only issuance or only
    /// consumption would create a split authority boundary.
    pub fn install_into_app_state(
        &self,
        state: &mut crate::http_server::AppState,
    ) -> Result<(), OwnerAuthorityAssemblyError> {
        if state.authority_service.is_some()
            || state.mission_service.is_some()
            || state.external_mutation_service.is_some()
            || state.autonomy_owner.is_some()
        {
            return Err(OwnerAuthorityAssemblyError::AppStateAlreadyConfigured);
        }
        let bound = Arc::clone(&state.session);
        let project_brains = Arc::clone(&state.project_brains);
        let reconciliation_registry = Arc::clone(&state.project_brains);
        let reconciliation_brain = Arc::clone(&state.session);
        let bound_for_reconciliation = Arc::clone(&state.session);
        let reconcile_promote: Arc<crate::external_mutation_service::BrainPromoteReconcilerV1> =
            Arc::new(move |request| {
                let runs_on_source_brain = request.runs_on_source_brain_actor();
                let requires_checkpoint_ack = request.requires_checkpoint_ack();
                let allows_resolved_actor_identity = request.allows_resolved_actor_identity();
                let failure_code = request.actor_failure_code();
                let (actor_brain, selected_project_root, bound) = if runs_on_source_brain {
                    let binding = reconciliation_registry
                        .resolve_external_mutation_actor_by_id(
                            Arc::clone(&bound_for_reconciliation),
                            &request.source_brain_id,
                        )
                        .map_err(|error| error.to_string())?;
                    if binding.brain_id != request.reconciliation_brain_id
                        && !allows_resolved_actor_identity
                    {
                        return Err(format!(
                            "reconciliation actor mismatch: expected '{}', observed '{}'",
                            request.reconciliation_brain_id, binding.brain_id
                        ));
                    }
                    (binding.brain, binding.selected_project_root, binding.bound)
                } else {
                    (Arc::clone(&reconciliation_brain), None, true)
                };
                let actual_brain_id = if bound {
                    reconciliation_registry
                        .bound_brain_id_for_target(Arc::clone(&actor_brain))
                        .map_err(|error| error.to_string())?
                } else {
                    reconciliation_registry.brain_id_for(
                        selected_project_root
                            .as_deref()
                            .ok_or_else(|| "hosted actor project root is missing".to_string())?,
                    )
                };
                if actual_brain_id != request.reconciliation_brain_id
                    && !allows_resolved_actor_identity
                {
                    return Err(format!(
                        "reconciliation actor mismatch: expected '{}', observed '{}'",
                        request.reconciliation_brain_id, actual_brain_id
                    ));
                }
                if requires_checkpoint_ack {
                    reconciliation_registry
                        .execute_target_runtime_with_checkpoint_ack(
                            actor_brain,
                            selected_project_root.as_deref(),
                            bound,
                            move |session| {
                                request.execute(session).map_err(|detail| {
                                    crate::runtime_jobs::RuntimeJobFailure::new(
                                        failure_code,
                                        detail,
                                    )
                                })
                            },
                        )
                        .map(|(execution, ack)| execution.bind_checkpoint_ack(&ack))
                        .map_err(|error| error.to_string())
                } else {
                    reconciliation_registry
                        .execute_target_runtime(
                            actor_brain,
                            selected_project_root.as_deref(),
                            bound,
                            false,
                            move |session| {
                                request.execute(session).map_err(|detail| {
                                    crate::runtime_jobs::RuntimeJobFailure::new(
                                        failure_code,
                                        detail,
                                    )
                                })
                            },
                        )
                        .map_err(|error| error.to_string())
                }
            });
        self.external_mutation_service.recover_for_boot(
            |actor_brain_id| {
                project_brains
                    .resolve_external_mutation_actor_by_id(Arc::clone(&bound), actor_brain_id)
                    .map(|binding| binding.brain)
                    .map_err(|error| error.to_string())
            },
            reconcile_promote,
        )?;
        let conservation = self.external_mutation_service.conservation_scan()?;
        if !conservation.anomalies.is_empty() {
            return Err(OwnerAuthorityAssemblyError::ExternalMutation(
                crate::external_mutation_service::ExternalMutationError::refused(
                    "external_mutation_conservation_failed",
                    conservation.anomalies.join("; "),
                ),
            ));
        }
        state.authority_service = Some(Arc::clone(&self.authority_service));
        state.mission_service = Some(Arc::clone(&self.mission_service));
        state.external_mutation_service = Some(Arc::clone(&self.external_mutation_service));
        state.autonomy_owner = self.autonomy_owner.as_ref().map(Arc::clone);
        Ok(())
    }
}

#[cfg(feature = "serve")]
pub fn install_owner_authority_for_http_boot_v1(
    state: &mut crate::http_server::AppState,
    assembly: Option<&OwnerAuthorityAssemblyV1>,
    requirement: OwnerAuthorityBootRequirementV1,
) -> Result<OwnerAuthorityInstallationStatusV1, OwnerAuthorityAssemblyError> {
    match assembly {
        Some(assembly) => {
            assembly.install_into_app_state(state)?;
            Ok(OwnerAuthorityInstallationStatusV1::Installed)
        }
        None if requirement == OwnerAuthorityBootRequirementV1::Required => {
            Err(OwnerAuthorityAssemblyError::ProductionAdapterNotInstalled)
        }
        None => Ok(OwnerAuthorityInstallationStatusV1::NotInstalled),
    }
}

#[derive(Debug)]
pub enum OwnerAuthorityAssemblyError {
    SecurityAssuranceRequired,
    RuntimeAssuranceRequired,
    WalCryptoAssuranceRequired,
    JournalHeadAssuranceRequired,
    AutonomyAssuranceRequired,
    ProductionAdapterNotInstalled,
    AppStateAlreadyConfigured,
    Security(OwnerSecurityConfigError),
    Runtime(AuthorityRuntimeError),
    Mission(MissionServiceTransportError),
    ExternalMutation(crate::external_mutation_service::ExternalMutationError),
}

impl fmt::Display for OwnerAuthorityAssemblyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SecurityAssuranceRequired => formatter.write_str(
                "production assembly requires a hardware-protected owner-security config root",
            ),
            Self::RuntimeAssuranceRequired => formatter.write_str(
                "production assembly requires a hardware-protected AuthorityRuntime epoch root",
            ),
            Self::WalCryptoAssuranceRequired => formatter.write_str(
                "production assembly requires a cryptographic AuthorityWAL signer/verifier",
            ),
            Self::JournalHeadAssuranceRequired => formatter.write_str(
                "production assembly requires a hardware-protected broker/WAL anti-rollback head",
            ),
            Self::AutonomyAssuranceRequired => formatter.write_str(
                "production autonomous assembly requires a protected-production G9 owner",
            ),
            Self::ProductionAdapterNotInstalled => formatter.write_str(
                "owner authority production adapters are NOT_INSTALLED; required G2/G3 boot refused",
            ),
            Self::AppStateAlreadyConfigured => formatter
                .write_str("AppState already contains a partial or complete authority assembly"),
            Self::Security(error) => write!(formatter, "owner security assembly: {error}"),
            Self::Runtime(error) => write!(formatter, "authority runtime assembly: {error}"),
            Self::Mission(error) => write!(formatter, "MissionService assembly: {error}"),
            Self::ExternalMutation(error) => {
                write!(formatter, "external mutation recovery: {error}")
            }
        }
    }
}

impl Error for OwnerAuthorityAssemblyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Security(error) => Some(error),
            Self::Runtime(error) => Some(error),
            Self::Mission(error) => Some(error),
            Self::ExternalMutation(error) => Some(error),
            _ => None,
        }
    }
}

impl From<OwnerSecurityConfigError> for OwnerAuthorityAssemblyError {
    fn from(error: OwnerSecurityConfigError) -> Self {
        Self::Security(error)
    }
}

impl From<AuthorityRuntimeError> for OwnerAuthorityAssemblyError {
    fn from(error: AuthorityRuntimeError) -> Self {
        Self::Runtime(error)
    }
}

impl From<MissionServiceTransportError> for OwnerAuthorityAssemblyError {
    fn from(error: MissionServiceTransportError) -> Self {
        Self::Mission(error)
    }
}

impl From<crate::external_mutation_service::ExternalMutationError> for OwnerAuthorityAssemblyError {
    fn from(error: crate::external_mutation_service::ExternalMutationError) -> Self {
        Self::ExternalMutation(error)
    }
}

/// Assemble the production G2/G3 owner without consulting environment key
/// material or selecting a software fallback. The caller must inject two
/// independent protected roots and the production AuthorityWAL signer/verifier.
pub struct ProductionOwnerAuthorityInputsV1 {
    pub loaded_security: LoadedOwnerSecurityConfigV1,
    pub startup: OwnerAuthorityStartupV1,
    pub authority_epoch_backend: Box<dyn ProtectedEpochBackend>,
    pub mission_config: MissionServiceConfigV1,
    pub owner_clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    pub wal_record_crypto: Arc<dyn AuthorityWalRecordCrypto>,
    pub protected_journal_head: SharedProtectedJournalHeadBackendV1,
}

pub fn assemble_production_owner_authority_v1(
    inputs: ProductionOwnerAuthorityInputsV1,
) -> Result<OwnerAuthorityAssemblyV1, OwnerAuthorityAssemblyError> {
    assemble_production_owner_authority_internal_v1(inputs, None)
}

/// Assemble G2/G3 and the protected G9 owner as one served-owner boundary.
/// The exact same trait object is installed into the mutation runtime and the
/// read-only manifest path; a test-assurance autonomy adapter is refused.
pub fn assemble_production_owner_authority_with_autonomy_v1(
    inputs: ProductionOwnerAuthorityInputsV1,
    autonomy_owner: Arc<dyn AutonomyAdmissionOwner>,
) -> Result<OwnerAuthorityAssemblyV1, OwnerAuthorityAssemblyError> {
    assemble_production_owner_authority_internal_v1(inputs, Some(autonomy_owner))
}

fn assemble_production_owner_authority_internal_v1(
    inputs: ProductionOwnerAuthorityInputsV1,
    autonomy_owner: Option<Arc<dyn AutonomyAdmissionOwner>>,
) -> Result<OwnerAuthorityAssemblyV1, OwnerAuthorityAssemblyError> {
    let ProductionOwnerAuthorityInputsV1 {
        loaded_security,
        startup,
        authority_epoch_backend,
        mission_config,
        owner_clock,
        wal_record_crypto,
        protected_journal_head,
    } = inputs;
    let owner_now_ms = owner_clock();
    // The loaded object is opaque to downstream callers, but assembly still
    // revalidates the complete immutable snapshot at its trust boundary.
    loaded_security.config.validate(owner_now_ms)?;
    loaded_security.revalidate_durable_roots()?;
    if loaded_security.root_assurance
        != OwnerSecurityConfigRootAssuranceV1::HardwareProtectedAttested
    {
        return Err(OwnerAuthorityAssemblyError::SecurityAssuranceRequired);
    }
    // Assurance checks are preflight-only: refusing a downgrade must not
    // create an AuthorityRuntime root or any other durable artifact first.
    if authority_epoch_backend.assurance() != ProtectedEpochAssurance::HardwareProtectedAttested {
        return Err(OwnerAuthorityAssemblyError::RuntimeAssuranceRequired);
    }
    if wal_record_crypto.assurance() != AuthorityWalCryptoAssurance::ProductionCryptographic {
        return Err(OwnerAuthorityAssemblyError::WalCryptoAssuranceRequired);
    }
    if protected_journal_head.lock().assurance()
        != ProtectedJournalHeadAssuranceV1::HardwareProtectedAttested
    {
        return Err(OwnerAuthorityAssemblyError::JournalHeadAssuranceRequired);
    }
    if autonomy_owner
        .as_ref()
        .is_some_and(|owner| owner.assurance() != AutonomyRuntimeAssurance::ProtectedProduction)
    {
        return Err(OwnerAuthorityAssemblyError::AutonomyAssuranceRequired);
    }
    let runtime_config = loaded_security.authority_runtime_config();
    let mut runtime = match startup {
        OwnerAuthorityStartupV1::BootstrapFrozen => {
            AuthorityRuntime::bootstrap(runtime_config, authority_epoch_backend)?
        }
        OwnerAuthorityStartupV1::OpenExisting => {
            AuthorityRuntime::open(runtime_config, authority_epoch_backend)?
        }
    };
    if let Some(owner) = autonomy_owner.as_ref() {
        runtime.install_autonomy_admission_owner(Arc::clone(owner))?;
        if startup == OwnerAuthorityStartupV1::OpenExisting {
            runtime.synchronize_autonomy_authority(owner_now_ms)?;
        }
    }
    if runtime.status()?.protected_epoch_assurance
        != ProtectedEpochAssurance::HardwareProtectedAttested
    {
        return Err(OwnerAuthorityAssemblyError::RuntimeAssuranceRequired);
    }
    let runtime = Arc::new(runtime);
    let external_mutation_journal_root = loaded_security
        .mission_service_root()
        .join("external-mutations");
    let (authority_service, mission_authority_provider, external_mutation_service) =
        owner_authority_components_with_external_mutation(
            OwnerAuthorityComponentInputsV1 {
                runtime: Arc::clone(&runtime),
                verification_keys: loaded_security.verification_keys(),
                session_roles: loaded_security.session_roles(),
                max_future_clock_skew_ms: loaded_security.max_future_clock_skew_ms(),
                receipt_crypto: Arc::clone(&wal_record_crypto),
                broker_config: loaded_security.authorization_broker_config(),
                linearization: OwnerAuthorityLinearizationV1::default(),
                protected_journal_head: Arc::clone(&protected_journal_head),
            },
            external_mutation_journal_root,
            Arc::clone(&owner_clock),
        );
    let mission_service = Arc::new(
        MissionServiceTransportFacade::open_with_production_wal_crypto(
            loaded_security.mission_service_root(),
            mission_config,
            mission_authority_provider.clone(),
            owner_clock,
            wal_record_crypto,
            protected_journal_head,
        )?,
    );
    Ok(OwnerAuthorityAssemblyV1 {
        loaded_security,
        authority_runtime: runtime,
        authority_service,
        mission_authority_provider,
        mission_service,
        external_mutation_service,
        autonomy_owner,
    })
}

pub struct OwnerSecurityConfigLoaderV1;

impl OwnerSecurityConfigLoaderV1 {
    /// Pin an already reviewed config. File publication and protected-root CAS
    /// are deliberately separate operator steps: a crash leaves a detectable
    /// fail-closed mismatch, never a guessed successful update.
    pub fn pin(
        path: impl AsRef<Path>,
        backend: &mut dyn ProtectedOwnerSecurityConfigRootBackendV1,
        owner_now_ms: u64,
    ) -> Result<OwnerSecurityConfigRootV1, OwnerSecurityConfigError> {
        let (_, config) = read_unpinned(path.as_ref(), owner_now_ms)?;
        let current = backend
            .read_latest()
            .map_err(OwnerSecurityConfigError::ProtectedRoot)?;
        match current.as_ref() {
            None if config.config_epoch != 1 || config.previous_config_digest.is_some() => {
                return Err(OwnerSecurityConfigError::Rollback {
                    detail: "initial protected root requires config epoch one".to_string(),
                });
            }
            Some(previous)
                if config.config_epoch != previous.config_epoch.saturating_add(1)
                    || config.previous_config_digest.as_deref()
                        != Some(previous.config_digest.as_str()) =>
            {
                return Err(OwnerSecurityConfigError::Rollback {
                    detail: "config does not extend the exact protected predecessor".to_string(),
                });
            }
            _ => {}
        }
        let next = OwnerSecurityConfigRootV1 {
            config_epoch: config.config_epoch,
            config_digest: config.config_digest,
        };
        backend
            .compare_and_advance(current.as_ref(), &next)
            .map_err(OwnerSecurityConfigError::ProtectedRoot)?;
        Ok(next)
    }

    pub fn load(
        path: impl AsRef<Path>,
        backend: &dyn ProtectedOwnerSecurityConfigRootBackendV1,
        owner_now_ms: u64,
    ) -> Result<LoadedOwnerSecurityConfigV1, OwnerSecurityConfigError> {
        Self::load_internal(path.as_ref(), backend, owner_now_ms, false)
    }

    pub fn load_production(
        path: impl AsRef<Path>,
        backend: &dyn ProtectedOwnerSecurityConfigRootBackendV1,
        owner_now_ms: u64,
    ) -> Result<LoadedOwnerSecurityConfigV1, OwnerSecurityConfigError> {
        Self::load_internal(path.as_ref(), backend, owner_now_ms, true)
    }

    fn load_internal(
        path: &Path,
        backend: &dyn ProtectedOwnerSecurityConfigRootBackendV1,
        owner_now_ms: u64,
        require_production: bool,
    ) -> Result<LoadedOwnerSecurityConfigV1, OwnerSecurityConfigError> {
        let (source_path, config) = read_unpinned(path, owner_now_ms)?;
        let assurance = backend.assurance();
        if require_production
            && assurance != OwnerSecurityConfigRootAssuranceV1::HardwareProtectedAttested
        {
            return Err(OwnerSecurityConfigError::ProductionAssuranceRequired);
        }
        let protected = backend
            .read_latest()
            .map_err(OwnerSecurityConfigError::ProtectedRoot)?
            .ok_or_else(|| OwnerSecurityConfigError::Rollback {
                detail: "owner security config has no protected root".to_string(),
            })?;
        if protected.config_epoch != config.config_epoch
            || protected.config_digest != config.config_digest
        {
            return Err(OwnerSecurityConfigError::Rollback {
                detail: "file epoch/digest differs from the protected owner root".to_string(),
            });
        }
        let base = source_path
            .parent()
            .ok_or_else(|| OwnerSecurityConfigError::Invalid {
                code: "owner_security_config_path_invalid",
                detail: source_path.display().to_string(),
            })?;
        let authority_runtime_root = resolve_root(base, &config.authority_runtime_root)?;
        let authorization_broker_root = resolve_root(base, &config.authorization_broker_root)?;
        let mission_service_root = resolve_root(base, &config.mission_service_root)?;
        for root in [
            &authority_runtime_root,
            &authorization_broker_root,
            &mission_service_root,
        ] {
            refuse_symlink_components_beneath(base, root)?;
        }
        Ok(LoadedOwnerSecurityConfigV1 {
            source_path,
            config,
            root_assurance: assurance,
            authority_runtime_root,
            authorization_broker_root,
            mission_service_root,
        })
    }
}

#[derive(Debug)]
pub enum OwnerSecurityConfigError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Canonical(m1nd_control::CanonicalError),
    Invalid { code: &'static str, detail: String },
    Symlink { path: PathBuf },
    Rollback { detail: String },
    ProtectedRoot(String),
    ProductionAssuranceRequired,
}

impl OwnerSecurityConfigError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "owner_security_config_io",
            Self::Json(_) => "owner_security_config_decode",
            Self::Canonical(_) => "owner_security_config_canonicalization",
            Self::Invalid { code, .. } => code,
            Self::Symlink { .. } => "owner_security_config_symlink_refused",
            Self::Rollback { .. } => "owner_security_config_rollback_detected",
            Self::ProtectedRoot(_) => "owner_security_config_protected_root_failed",
            Self::ProductionAssuranceRequired => "owner_security_config_hardware_root_required",
        }
    }
}

impl fmt::Display for OwnerSecurityConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "owner security config I/O: {error}"),
            Self::Json(error) => write!(formatter, "owner security config JSON: {error}"),
            Self::Canonical(error) => write!(formatter, "owner security config digest: {error}"),
            Self::Invalid { code, detail } => write!(formatter, "{code}: {detail}"),
            Self::Symlink { path } => {
                write!(
                    formatter,
                    "owner security config symlink refused: {}",
                    path.display()
                )
            }
            Self::Rollback { detail } => write!(formatter, "owner security rollback: {detail}"),
            Self::ProtectedRoot(detail) => {
                write!(formatter, "owner security protected root: {detail}")
            }
            Self::ProductionAssuranceRequired => write!(
                formatter,
                "production owner security config requires a hardware-protected attested root"
            ),
        }
    }
}

impl Error for OwnerSecurityConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Canonical(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for OwnerSecurityConfigError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for OwnerSecurityConfigError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<m1nd_control::CanonicalError> for OwnerSecurityConfigError {
    fn from(error: m1nd_control::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

fn read_unpinned(
    path: &Path,
    owner_now_ms: u64,
) -> Result<(PathBuf, OwnerSecurityConfigV1), OwnerSecurityConfigError> {
    let source_path = physical_source_path(path)?;
    refuse_symlink(&source_path)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options.open(&source_path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_OWNER_SECURITY_CONFIG_BYTES {
        return Err(OwnerSecurityConfigError::Invalid {
            code: "owner_security_config_file_invalid",
            detail: "config must be a bounded regular file".to_string(),
        });
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_OWNER_SECURITY_CONFIG_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_OWNER_SECURITY_CONFIG_BYTES {
        return Err(OwnerSecurityConfigError::Invalid {
            code: "owner_security_config_file_invalid",
            detail: "config exceeded its bounded read limit".to_string(),
        });
    }
    let config: OwnerSecurityConfigV1 = serde_json::from_slice(&bytes)?;
    config.validate(owner_now_ms)?;
    Ok((source_path, config))
}

fn normalized_relative_root(value: &str) -> Result<PathBuf, OwnerSecurityConfigError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(OwnerSecurityConfigError::Invalid {
            code: "owner_security_config_root_invalid",
            detail: value.to_string(),
        });
    }
    Ok(path.components().collect())
}

fn roots_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn resolve_root(base: &Path, relative: &str) -> Result<PathBuf, OwnerSecurityConfigError> {
    Ok(base.join(normalized_relative_root(relative)?))
}

fn absolute_lexical(path: &Path) -> Result<PathBuf, OwnerSecurityConfigError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

/// Resolve the containing directory to its physical identity before deriving
/// any durable root. The final config component is then opened with NOFOLLOW
/// on Unix, avoiding both lexical `/var` aliases and final-component links.
fn physical_source_path(path: &Path) -> Result<PathBuf, OwnerSecurityConfigError> {
    let lexical = absolute_lexical(path)?;
    let parent = lexical
        .parent()
        .ok_or_else(|| OwnerSecurityConfigError::Invalid {
            code: "owner_security_config_path_invalid",
            detail: lexical.display().to_string(),
        })?;
    let file_name = lexical
        .file_name()
        .ok_or_else(|| OwnerSecurityConfigError::Invalid {
            code: "owner_security_config_path_invalid",
            detail: lexical.display().to_string(),
        })?;
    let physical_parent = fs::canonicalize(parent)?;
    if !physical_parent.is_dir() {
        return Err(OwnerSecurityConfigError::Invalid {
            code: "owner_security_config_path_invalid",
            detail: "config parent is not a directory".to_string(),
        });
    }
    Ok(physical_parent.join(file_name))
}

fn refuse_symlink_components_beneath(
    base: &Path,
    target: &Path,
) -> Result<(), OwnerSecurityConfigError> {
    let relative = target
        .strip_prefix(base)
        .map_err(|_| OwnerSecurityConfigError::Invalid {
            code: "owner_security_config_root_escape",
            detail: target.display().to_string(),
        })?;
    let mut current = base.to_path_buf();
    let component_count = relative.components().count();
    for (index, component) in relative.components().enumerate() {
        let Component::Normal(component) = component else {
            return Err(OwnerSecurityConfigError::Invalid {
                code: "owner_security_config_root_invalid",
                detail: target.display().to_string(),
            });
        };
        current.push(component);
        match current.symlink_metadata() {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(OwnerSecurityConfigError::Symlink {
                        path: current.clone(),
                    });
                }
                if index + 1 < component_count && !metadata.is_dir() {
                    return Err(OwnerSecurityConfigError::Invalid {
                        code: "owner_security_config_root_component_not_directory",
                        detail: current.display().to_string(),
                    });
                }
                if index + 1 == component_count && !metadata.is_dir() {
                    return Err(OwnerSecurityConfigError::Invalid {
                        code: "owner_security_config_root_not_directory",
                        detail: current.display().to_string(),
                    });
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn refuse_symlink(path: &Path) -> Result<(), OwnerSecurityConfigError> {
    if path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(OwnerSecurityConfigError::Symlink {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use m1nd_control::{
        ActionEffectFloorV1, ActionId, ActionPolicyRuleV1, ActiveMode, AuthorityVariant, Effect,
        IdentityStatus, Ingress, ReachablePolicyTupleV1, RiskClass, VerificationKeyV1,
        ACTION_POLICY_REGISTRY_SCHEMA, ED25519_ALGORITHM, VERIFICATION_KEY_REGISTRY_SCHEMA,
    };
    use tempfile::TempDir;

    use super::*;

    const NOW: u64 = 100;
    const VALID_TEST_PUBLIC_KEY: &str =
        "5866666666666666666666666666666666666666666666666666666666666666";

    fn hash(label: &str) -> String {
        digest_canonical("owner-security-config-test-v1", &label).unwrap()
    }

    fn config(epoch: u64, previous: Option<String>) -> OwnerSecurityConfigV1 {
        let action = ActionId::new("mission.service.land_intent").unwrap();
        let effects = BTreeSet::from([Effect::Read]);
        let tuple = ReachablePolicyTupleV1 {
            ingress: Ingress::Mcp,
            action: action.clone(),
            active_mode: ActiveMode::HumanGated,
            subject_id: "owner-1".to_string(),
            authority_variant: AuthorityVariant::Ordinary,
            applicable_grant_id: None,
            applicable_tier: None,
            risk_class: RiskClass::Low,
        };
        let mut policy = ActionPolicyRegistryV1 {
            schema: ACTION_POLICY_REGISTRY_SCHEMA.to_string(),
            policy_version: "owner-security-test-v1".to_string(),
            reachable_tuples: vec![tuple.clone()],
            rules: vec![ActionPolicyRuleV1 {
                tuple,
                effects: effects.clone(),
            }],
            action_effect_floors: vec![ActionEffectFloorV1 {
                action,
                required_effects: effects,
            }],
            policy_digest: String::new(),
        };
        policy.seal().unwrap();
        let verification_keys = VerificationKeyRegistryV1 {
            schema: VERIFICATION_KEY_REGISTRY_SCHEMA.to_string(),
            registry_epoch: 1,
            keys: BTreeMap::from([(
                "owner-key-1".to_string(),
                VerificationKeyV1 {
                    key_id: "owner-key-1".to_string(),
                    subject_id: "owner-1".to_string(),
                    algorithm: ED25519_ALGORITHM.to_string(),
                    public_key: VALID_TEST_PUBLIC_KEY.to_string(),
                    created_at: 1,
                    activated_at: 2,
                    expires_at: None,
                    revoked_at: None,
                    rotated_at: None,
                    replacement_key_id: None,
                    status: IdentityStatus::Active,
                },
            )]),
        };
        let mut config = OwnerSecurityConfigV1 {
            schema: OWNER_SECURITY_CONFIG_SCHEMA.to_string(),
            config_epoch: epoch,
            previous_config_digest: previous,
            authority_runtime_root: "authority".to_string(),
            authorization_broker_root: "broker".to_string(),
            mission_service_root: "mission".to_string(),
            organism_id: "organism-1".to_string(),
            repo_id: "repo-1".to_string(),
            brain_id: "brain-1".to_string(),
            audience: "m1nd-runtime".to_string(),
            constitution_digest: hash("constitution"),
            constitution_epoch: 1,
            grants_digest: hash("grants"),
            policy_registry_digest: policy.policy_digest.clone(),
            policy_registry: policy,
            service_identities: BTreeMap::new(),
            safety_kernel_digest: hash("safety-kernel"),
            safety_actuator_identity_key_binary_policy_digest: hash("safety-actuator"),
            verification_keys,
            session_roles: BTreeMap::from([("owner-1".to_string(), Role::Author)]),
            max_future_clock_skew_ms: 10,
            authorization_reservation_ttl_ms: 1_000,
            authorization_terminal_retention_ms: 2_000,
            config_digest: String::new(),
        };
        config.seal().unwrap();
        config
    }

    fn write(path: &Path, config: &OwnerSecurityConfigV1) {
        fs::write(path, serde_json::to_vec(config).unwrap()).unwrap();
    }

    #[derive(Default)]
    struct TestHardwareConfigRoot {
        state: Option<OwnerSecurityConfigRootV1>,
    }

    impl ProtectedOwnerSecurityConfigRootBackendV1 for TestHardwareConfigRoot {
        fn assurance(&self) -> OwnerSecurityConfigRootAssuranceV1 {
            OwnerSecurityConfigRootAssuranceV1::HardwareProtectedAttested
        }

        fn read_latest(&self) -> Result<Option<OwnerSecurityConfigRootV1>, String> {
            Ok(self.state.clone())
        }

        fn compare_and_advance(
            &mut self,
            expected: Option<&OwnerSecurityConfigRootV1>,
            next: &OwnerSecurityConfigRootV1,
        ) -> Result<(), String> {
            if self.state.as_ref() != expected {
                return Err("test config-root CAS mismatch".to_string());
            }
            self.state = Some(next.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct TestHardwareRuntimeEpoch {
        state: Option<crate::authority_runtime::ProtectedEpochSnapshotV1>,
    }

    impl ProtectedEpochBackend for TestHardwareRuntimeEpoch {
        fn assurance(&self) -> ProtectedEpochAssurance {
            ProtectedEpochAssurance::HardwareProtectedAttested
        }

        fn read_latest(
            &self,
        ) -> Result<Option<crate::authority_runtime::ProtectedEpochSnapshotV1>, String> {
            Ok(self.state.clone())
        }

        fn compare_and_advance(
            &mut self,
            expected: Option<&crate::authority_runtime::ProtectedEpochSnapshotV1>,
            next: &crate::authority_runtime::ProtectedEpochSnapshotV1,
        ) -> Result<(), String> {
            if self.state.as_ref() != expected {
                return Err("test runtime-root CAS mismatch".to_string());
            }
            self.state = Some(next.clone());
            Ok(())
        }
    }

    struct TestProductionWalCrypto;

    impl AuthorityWalRecordCrypto for TestProductionWalCrypto {
        fn assurance(&self) -> AuthorityWalCryptoAssurance {
            AuthorityWalCryptoAssurance::ProductionCryptographic
        }

        fn issuer(&self) -> &str {
            "test-production-owner"
        }

        fn key_id(&self) -> &str {
            "test-production-key"
        }

        fn algorithm(&self) -> &str {
            "TEST_PRODUCTION_ASSURANCE_FIXTURE"
        }

        fn sign(&self, _canonical_record_message: &[u8]) -> Result<String, String> {
            Err("preflight fixture must never sign".to_string())
        }

        fn verify(&self, _canonical_record_message: &[u8], _signature: &str) -> Result<(), String> {
            Err("preflight fixture must never verify".to_string())
        }
    }

    #[derive(Default)]
    struct TestHardwareJournalHead;

    impl crate::protected_journal_head::ProtectedJournalHeadBackendV1 for TestHardwareJournalHead {
        fn assurance(&self) -> ProtectedJournalHeadAssuranceV1 {
            ProtectedJournalHeadAssuranceV1::HardwareProtectedAttested
        }

        fn read_latest(
            &self,
            _domain: &str,
        ) -> Result<Option<crate::protected_journal_head::ProtectedJournalHeadSnapshotV1>, String>
        {
            Err("preflight fixture must never read".to_string())
        }

        fn compare_and_advance(
            &mut self,
            _domain: &str,
            _expected: Option<&crate::protected_journal_head::ProtectedJournalHeadSnapshotV1>,
            _next: &crate::protected_journal_head::ProtectedJournalHeadSnapshotV1,
        ) -> Result<(), String> {
            Err("preflight fixture must never advance".to_string())
        }
    }

    struct TestSoftwareAutonomyOwner;

    impl crate::autonomy_manifest::AutonomyManifestReader for TestSoftwareAutonomyOwner {
        fn read_projection(
            &self,
            _observed_at: u64,
        ) -> Result<
            crate::autonomy_manifest::AutonomyManifestProjectionV1,
            crate::autonomy_manifest::AutonomyManifestProjectionError,
        > {
            Err(crate::autonomy_manifest::AutonomyManifestProjectionError::ProtectedRootMissing)
        }
    }

    impl AutonomyAdmissionOwner for TestSoftwareAutonomyOwner {
        fn assurance(&self) -> AutonomyRuntimeAssurance {
            AutonomyRuntimeAssurance::SoftwareTestOnlyNotProduction
        }

        fn admit(
            &self,
            _evidence: &crate::autonomy_manifest::AutonomyAuthorityEvidenceV1,
            _now_ms: u64,
        ) -> Result<
            crate::autonomy_manifest::AutonomyAdmissionOutcomeV1,
            crate::autonomy_manifest::AutonomyManifestProjectionError,
        > {
            Err(crate::autonomy_manifest::AutonomyManifestProjectionError::ProtectedRootMissing)
        }
    }

    #[test]
    fn protected_pin_load_and_component_projection_are_exact() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("owner-security.json");
        let config = config(1, None);
        write(&path, &config);
        let mut protected = SoftwareTestOwnerSecurityConfigRootBackendV1::new();
        OwnerSecurityConfigLoaderV1::pin(&path, &mut protected, NOW).unwrap();
        let loaded = OwnerSecurityConfigLoaderV1::load(&path, &protected, NOW).unwrap();
        assert_eq!(loaded.config.config_digest, config.config_digest);
        let physical_temp = fs::canonicalize(temp.path()).unwrap();
        assert_eq!(
            loaded.authority_runtime_config().root,
            physical_temp.join("authority")
        );
        assert_eq!(
            loaded.authorization_broker_config().root,
            physical_temp.join("broker")
        );
        assert_eq!(loaded.mission_service_root(), physical_temp.join("mission"));
        assert_eq!(
            OwnerSecurityConfigLoaderV1::load_production(&path, &protected, NOW)
                .unwrap_err()
                .code(),
            "owner_security_config_hardware_root_required"
        );
        assert!(matches!(
            assemble_production_owner_authority_v1(ProductionOwnerAuthorityInputsV1 {
                loaded_security: loaded,
                startup: OwnerAuthorityStartupV1::BootstrapFrozen,
                authority_epoch_backend: Box::new(
                    crate::authority_runtime::SoftwareTestProtectedEpochBackend::new(),
                ),
                mission_config: crate::mission_service_tests::config(),
                owner_clock: Arc::new(|| NOW),
                wal_record_crypto: Arc::new(
                    crate::authority_wal::SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(
                        b"explicit-test-only",
                    ),
                ),
                protected_journal_head: crate::protected_journal_head::SoftwareTestProtectedJournalHeadBackendV1::new()
                    .shared(),
            }),
            Err(OwnerAuthorityAssemblyError::SecurityAssuranceRequired)
        ));
    }

    #[test]
    fn rollback_tamper_and_non_extending_update_fail_closed() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("owner-security.json");
        let first = config(1, None);
        write(&path, &first);
        let mut protected = SoftwareTestOwnerSecurityConfigRootBackendV1::new();
        OwnerSecurityConfigLoaderV1::pin(&path, &mut protected, NOW).unwrap();

        let mut second = config(2, Some(first.config_digest.clone()));
        second.constitution_epoch = 2;
        second.seal().unwrap();
        write(&path, &second);
        OwnerSecurityConfigLoaderV1::pin(&path, &mut protected, NOW).unwrap();
        write(&path, &first);
        assert_eq!(
            OwnerSecurityConfigLoaderV1::load(&path, &protected, NOW)
                .unwrap_err()
                .code(),
            "owner_security_config_rollback_detected"
        );

        let wrong = config(3, Some(hash("not-the-protected-predecessor")));
        write(&path, &wrong);
        assert_eq!(
            OwnerSecurityConfigLoaderV1::pin(&path, &mut protected, NOW)
                .unwrap_err()
                .code(),
            "owner_security_config_rollback_detected"
        );

        let mut tampered = second;
        tampered.audience = "tampered".to_string();
        write(&path, &tampered);
        assert_eq!(
            OwnerSecurityConfigLoaderV1::load(&path, &protected, NOW)
                .unwrap_err()
                .code(),
            "owner_security_config_digest_mismatch"
        );
    }

    #[test]
    fn production_assurance_preflight_refuses_without_creating_durable_roots() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("owner-security.json");
        write(&path, &config(1, None));
        let mut protected = TestHardwareConfigRoot::default();
        OwnerSecurityConfigLoaderV1::pin(&path, &mut protected, NOW).unwrap();
        let loaded = OwnerSecurityConfigLoaderV1::load_production(&path, &protected, NOW).unwrap();
        let authority_root = loaded.authority_runtime_config().root;
        let broker_root = loaded.authorization_broker_config().root;
        let mission_root = loaded.mission_service_root().to_path_buf();
        let software_wal = || {
            Arc::new(
                crate::authority_wal::SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(
                    b"explicit-test-only",
                ),
            ) as Arc<dyn AuthorityWalRecordCrypto>
        };

        let runtime_error =
            assemble_production_owner_authority_v1(ProductionOwnerAuthorityInputsV1 {
                loaded_security: loaded.clone(),
                startup: OwnerAuthorityStartupV1::BootstrapFrozen,
                authority_epoch_backend: Box::new(
                    crate::authority_runtime::SoftwareTestProtectedEpochBackend::new(),
                ),
                mission_config: crate::mission_service_tests::config(),
                owner_clock: Arc::new(|| NOW),
                wal_record_crypto: software_wal(),
                protected_journal_head:
                    crate::protected_journal_head::SoftwareTestProtectedJournalHeadBackendV1::new()
                        .shared(),
            })
            .err()
            .expect("software runtime assurance must be refused before bootstrap");
        assert!(matches!(
            runtime_error,
            OwnerAuthorityAssemblyError::RuntimeAssuranceRequired
        ));
        assert!(!authority_root.exists());
        assert!(!broker_root.exists());
        assert!(!mission_root.exists());

        let wal_error = assemble_production_owner_authority_v1(ProductionOwnerAuthorityInputsV1 {
            loaded_security: loaded,
            startup: OwnerAuthorityStartupV1::BootstrapFrozen,
            authority_epoch_backend: Box::new(TestHardwareRuntimeEpoch::default()),
            mission_config: crate::mission_service_tests::config(),
            owner_clock: Arc::new(|| NOW),
            wal_record_crypto: software_wal(),
            protected_journal_head:
                crate::protected_journal_head::SoftwareTestProtectedJournalHeadBackendV1::new()
                    .shared(),
        })
        .err()
        .expect("software WAL assurance must be refused before bootstrap");
        assert!(matches!(
            wal_error,
            OwnerAuthorityAssemblyError::WalCryptoAssuranceRequired
        ));
        assert!(!authority_root.exists());
        assert!(!broker_root.exists());
        assert!(!mission_root.exists());
    }

    #[test]
    fn production_autonomy_assembly_refuses_software_g9_before_creating_roots() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("owner-security.json");
        write(&path, &config(1, None));
        let mut protected = TestHardwareConfigRoot::default();
        OwnerSecurityConfigLoaderV1::pin(&path, &mut protected, NOW).unwrap();
        let loaded = OwnerSecurityConfigLoaderV1::load_production(&path, &protected, NOW).unwrap();
        let authority_root = loaded.authority_runtime_config().root;
        let broker_root = loaded.authorization_broker_config().root;
        let mission_root = loaded.mission_service_root().to_path_buf();
        let protected_head: SharedProtectedJournalHeadBackendV1 =
            Arc::new(Mutex::new(Box::new(TestHardwareJournalHead)));

        let error = assemble_production_owner_authority_with_autonomy_v1(
            ProductionOwnerAuthorityInputsV1 {
                loaded_security: loaded,
                startup: OwnerAuthorityStartupV1::BootstrapFrozen,
                authority_epoch_backend: Box::new(TestHardwareRuntimeEpoch::default()),
                mission_config: crate::mission_service_tests::config(),
                owner_clock: Arc::new(|| NOW),
                wal_record_crypto: Arc::new(TestProductionWalCrypto),
                protected_journal_head: protected_head,
            },
            Arc::new(TestSoftwareAutonomyOwner),
        )
        .err()
        .expect("software G9 assurance must be refused during preflight");
        assert!(matches!(
            error,
            OwnerAuthorityAssemblyError::AutonomyAssuranceRequired
        ));
        assert!(!authority_root.exists());
        assert!(!broker_root.exists());
        assert!(!mission_root.exists());
    }

    #[cfg(unix)]
    #[test]
    fn config_and_resolved_root_symlinks_are_refused() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let real = temp.path().join("real.json");
        write(&real, &config(1, None));
        let linked = temp.path().join("linked.json");
        symlink(&real, &linked).unwrap();
        let mut protected = SoftwareTestOwnerSecurityConfigRootBackendV1::new();
        assert_eq!(
            OwnerSecurityConfigLoaderV1::pin(&linked, &mut protected, NOW)
                .unwrap_err()
                .code(),
            "owner_security_config_symlink_refused"
        );

        OwnerSecurityConfigLoaderV1::pin(&real, &mut protected, NOW).unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, temp.path().join("authority")).unwrap();
        assert_eq!(
            OwnerSecurityConfigLoaderV1::load(&real, &protected, NOW)
                .unwrap_err()
                .code(),
            "owner_security_config_symlink_refused"
        );
    }

    #[test]
    fn session_roles_and_durable_roots_are_closed_owner_configuration() {
        let mut missing_role = config(1, None);
        missing_role.session_roles.clear();
        missing_role.seal().unwrap();
        assert_eq!(
            missing_role.validate(NOW).unwrap_err().code(),
            "owner_security_session_roles_invalid"
        );

        let mut service_role = config(1, None);
        service_role
            .session_roles
            .insert("owner-1".to_string(), Role::MissionService);
        service_role.seal().unwrap();
        assert_eq!(
            service_role.validate(NOW).unwrap_err().code(),
            "owner_security_session_roles_invalid"
        );

        let mut curdir = config(1, None);
        curdir.authority_runtime_root = "./authority".to_string();
        curdir.seal().unwrap();
        assert_eq!(
            curdir.validate(NOW).unwrap_err().code(),
            "owner_security_config_root_invalid"
        );

        let mut nested = config(1, None);
        nested.authorization_broker_root = "authority/broker".to_string();
        nested.seal().unwrap();
        assert_eq!(
            nested.validate(NOW).unwrap_err().code(),
            "owner_security_config_roots_overlap"
        );
    }

    #[cfg(unix)]
    #[test]
    fn intermediate_durable_root_symlink_is_refused() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let path = temp.path().join("owner-security.json");
        let mut configured = config(1, None);
        configured.authority_runtime_root = "authority-parent/runtime".to_string();
        configured.seal().unwrap();
        write(&path, &configured);
        let outside = TempDir::new().unwrap();
        symlink(outside.path(), temp.path().join("authority-parent")).unwrap();

        let mut protected = SoftwareTestOwnerSecurityConfigRootBackendV1::new();
        OwnerSecurityConfigLoaderV1::pin(&path, &mut protected, NOW).unwrap();
        assert_eq!(
            OwnerSecurityConfigLoaderV1::load(&path, &protected, NOW)
                .unwrap_err()
                .code(),
            "owner_security_config_symlink_refused"
        );
    }
}
