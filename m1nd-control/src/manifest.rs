use std::collections::{BTreeMap, BTreeSet};

use crate::{digest_canonical, CanonicalError, OpaqueSignature};
use serde::{Deserialize, Serialize};

pub const ORGANISM_MANIFEST_SCHEMA: &str = "m1nd-organism-manifest-v1";
pub const MANIFEST_DIGEST_DOMAIN: &str = "m1nd-organism-manifest-v1";

pub const SOURCE_AUTHORITY_ID: &str = "source";
pub const RUNTIME_BINARY_AUTHORITY_ID: &str = "runtime_binary";
pub const GRAPH_AUTHORITY_ID: &str = "graph";
pub const ARCHITECTURE_AUTHORITY_ID: &str = "architecture";
pub const UI_BUNDLE_AUTHORITY_ID: &str = "ui_bundle";
pub const RELEASE_AUTHORITY_ID: &str = "release_candidate";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFact {
    pub commit: String,
    pub dirty: bool,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFact {
    pub owner_id: String,
    pub binary_version: String,
    pub binary_sha256: String,
    pub started_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphFact {
    pub generation: u64,
    pub snapshot_sha256: String,
    pub node_count: u64,
    pub edge_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchitectureFact {
    pub store_version: u64,
    pub skeleton_digest: String,
    pub ratification_state: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiFact {
    pub bundle_version: String,
    pub bundle_sha256: String,
    pub mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilitiesFact {
    pub policy_version: String,
    pub enabled_effects: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyFact {
    pub supported_modes: BTreeSet<String>,
    pub mechanically_proven_modes: BTreeSet<String>,
    pub active_mode: String,
    pub activation_receipt_id: String,
    pub constitution_digest: String,
    pub constitution_epoch: u64,
    pub safety_kernel_digest: String,
    pub autonomy_epoch: u64,
    pub grants_digest: String,
    pub quorum_policy_digest: String,
    pub max_effective_tier_projection: String,
    pub issuance_frozen: bool,
    pub sentinel_safety_state: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemasFact {
    pub mission: String,
    pub receipt: String,
    pub checkpoint: String,
    pub light: String,
    pub system_blocks: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorityFreshness {
    Fresh,
    Stale,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorityStatus {
    Available,
    Degraded,
    Unavailable,
    Drift,
    Unknown,
}

/// One observation of an authoritative store.
///
/// `revision` and `digest` are deliberately empty for an unavailable/unknown
/// authority. This prevents a stale value from being restamped as current.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityFact {
    pub revision: String,
    pub digest: String,
    pub observed_at: u64,
    pub freshness: AuthorityFreshness,
    pub status: AuthorityStatus,
}

impl AuthorityFact {
    pub fn available(
        revision: impl Into<String>,
        digest: impl Into<String>,
        observed_at: u64,
    ) -> Self {
        Self {
            revision: revision.into(),
            digest: digest.into(),
            observed_at,
            freshness: AuthorityFreshness::Fresh,
            status: AuthorityStatus::Available,
        }
    }

    pub fn unavailable(observed_at: u64) -> Self {
        Self {
            revision: String::new(),
            digest: String::new(),
            observed_at,
            freshness: AuthorityFreshness::Unknown,
            status: AuthorityStatus::Unavailable,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseProvenanceFact {
    pub release_candidate_digest: String,
    pub signature: OpaqueSignature,
}

/// Canonical identity/coherence projection from PRD 6.1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrganismManifestV1 {
    pub schema: String,
    pub organism_id: String,
    pub repo_id: String,
    pub brain_id: String,
    pub project_root_fingerprint: String,
    pub source: SourceFact,
    pub runtime: RuntimeFact,
    pub graph: GraphFact,
    pub architecture: ArchitectureFact,
    pub ui: UiFact,
    pub capabilities: CapabilitiesFact,
    pub autonomy: AutonomyFact,
    pub schemas: SchemasFact,
    pub authorities: BTreeMap<String, AuthorityFact>,
    pub release_provenance: ReleaseProvenanceFact,
    pub generated_at: u64,
    pub manifest_sha256: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ManifestCoherence {
    Coherent,
    Drift,
    Degraded,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ManifestIssueKind {
    Drift,
    Degraded,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestIssue {
    pub kind: ManifestIssueKind,
    pub authority_id: Option<String>,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestVerification {
    pub coherence: ManifestCoherence,
    pub computed_manifest_sha256: String,
    pub issues: Vec<ManifestIssue>,
}

impl ManifestVerification {
    pub fn is_coherent(&self) -> bool {
        self.coherence == ManifestCoherence::Coherent
    }
}

impl OrganismManifestV1 {
    /// Compute the self digest while omitting only `manifest_sha256`.
    pub fn compute_manifest_sha256(&self) -> Result<String, CanonicalError> {
        let mut value = serde_json::to_value(self)?;
        let object = value
            .as_object_mut()
            .expect("OrganismManifestV1 always serializes as an object");
        object.remove("manifest_sha256");
        digest_canonical(MANIFEST_DIGEST_DOMAIN, &value)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.manifest_sha256 = self.compute_manifest_sha256()?;
        Ok(())
    }

    /// Verify the projection without treating it as an authority.
    ///
    /// Precedence is `DRIFT > UNKNOWN > DEGRADED > COHERENT`: known divergence
    /// cannot be hidden by an unavailable store, and absence cannot be reported
    /// as merely stale.
    pub fn verify(&self) -> Result<ManifestVerification, CanonicalError> {
        let computed_manifest_sha256 = self.compute_manifest_sha256()?;
        let mut issues = Vec::new();

        if self.schema != ORGANISM_MANIFEST_SCHEMA {
            push_issue(
                &mut issues,
                ManifestIssueKind::Drift,
                None,
                format!(
                    "schema '{}' is not '{ORGANISM_MANIFEST_SCHEMA}'",
                    self.schema
                ),
            );
        }
        if self.manifest_sha256 != computed_manifest_sha256 {
            push_issue(
                &mut issues,
                ManifestIssueKind::Drift,
                None,
                "manifest_sha256 does not match canonical manifest bytes",
            );
        }
        if self.source.dirty {
            push_issue(
                &mut issues,
                ManifestIssueKind::Drift,
                Some(SOURCE_AUTHORITY_ID),
                "source is dirty",
            );
        }

        for (field, value) in [
            ("source.commit", self.source.commit.as_str()),
            ("runtime.binary_sha256", self.runtime.binary_sha256.as_str()),
            ("graph.snapshot_sha256", self.graph.snapshot_sha256.as_str()),
            (
                "architecture.skeleton_digest",
                self.architecture.skeleton_digest.as_str(),
            ),
            ("ui.bundle_sha256", self.ui.bundle_sha256.as_str()),
            (
                "release_provenance.release_candidate_digest",
                self.release_provenance.release_candidate_digest.as_str(),
            ),
        ] {
            if value.is_empty() {
                push_issue(
                    &mut issues,
                    ManifestIssueKind::Drift,
                    None,
                    format!("required digest '{field}' is absent"),
                );
            }
        }

        if self.source.version != self.runtime.binary_version
            || self.source.version != self.ui.bundle_version
        {
            push_issue(
                &mut issues,
                ManifestIssueKind::Drift,
                None,
                format!(
                    "source/binary/bundle versions diverge: source={}, binary={}, bundle={}",
                    self.source.version, self.runtime.binary_version, self.ui.bundle_version
                ),
            );
        }

        if !self
            .autonomy
            .supported_modes
            .contains(&self.autonomy.active_mode)
        {
            push_issue(
                &mut issues,
                ManifestIssueKind::Drift,
                Some("autonomy"),
                "active_mode is not listed in supported_modes",
            );
        }
        if !self
            .autonomy
            .mechanically_proven_modes
            .is_subset(&self.autonomy.supported_modes)
        {
            push_issue(
                &mut issues,
                ManifestIssueKind::Drift,
                Some("autonomy"),
                "mechanically_proven_modes is not a subset of supported_modes",
            );
        }

        for (authority_id, fact) in &self.authorities {
            assess_authority_fact(authority_id, fact, &mut issues);
        }

        let projections = [
            (
                SOURCE_AUTHORITY_ID,
                self.source.version.clone(),
                self.source.commit.clone(),
            ),
            (
                RUNTIME_BINARY_AUTHORITY_ID,
                // The authority revision is the exact source commit captured
                // at build time. This binds a same-version binary to source
                // without extending the ratified RuntimeFact wire schema.
                self.source.commit.clone(),
                self.runtime.binary_sha256.clone(),
            ),
            (
                GRAPH_AUTHORITY_ID,
                self.graph.generation.to_string(),
                self.graph.snapshot_sha256.clone(),
            ),
            (
                ARCHITECTURE_AUTHORITY_ID,
                self.architecture.store_version.to_string(),
                self.architecture.skeleton_digest.clone(),
            ),
            (
                UI_BUNDLE_AUTHORITY_ID,
                self.ui.bundle_version.clone(),
                self.ui.bundle_sha256.clone(),
            ),
            (
                RELEASE_AUTHORITY_ID,
                self.source.version.clone(),
                self.release_provenance.release_candidate_digest.clone(),
            ),
        ];

        for (authority_id, expected_revision, expected_digest) in projections {
            let Some(fact) = self.authorities.get(authority_id) else {
                push_issue(
                    &mut issues,
                    ManifestIssueKind::Unknown,
                    Some(authority_id),
                    "authority observation is missing",
                );
                continue;
            };
            if matches!(
                fact.status,
                AuthorityStatus::Unavailable | AuthorityStatus::Unknown
            ) {
                continue;
            }
            if fact.revision != expected_revision {
                push_issue(
                    &mut issues,
                    ManifestIssueKind::Drift,
                    Some(authority_id),
                    format!(
                        "authority revision '{}' differs from projected revision '{}'",
                        fact.revision, expected_revision
                    ),
                );
            }
            if fact.digest != expected_digest {
                push_issue(
                    &mut issues,
                    ManifestIssueKind::Drift,
                    Some(authority_id),
                    "authority digest differs from projected digest",
                );
            }
        }

        if self.release_provenance.signature.is_empty() {
            push_issue(
                &mut issues,
                ManifestIssueKind::Unknown,
                Some(RELEASE_AUTHORITY_ID),
                "release provenance signature is absent; G1 does not synthesize one",
            );
        }

        for (field, value) in [
            ("organism_id", self.organism_id.as_str()),
            ("repo_id", self.repo_id.as_str()),
            ("brain_id", self.brain_id.as_str()),
            (
                "project_root_fingerprint",
                self.project_root_fingerprint.as_str(),
            ),
            ("runtime.owner_id", self.runtime.owner_id.as_str()),
        ] {
            if value.is_empty() {
                push_issue(
                    &mut issues,
                    ManifestIssueKind::Unknown,
                    None,
                    format!("identity fact '{field}' is absent"),
                );
            }
        }

        let coherence = if issues
            .iter()
            .any(|issue| issue.kind == ManifestIssueKind::Drift)
        {
            ManifestCoherence::Drift
        } else if issues
            .iter()
            .any(|issue| issue.kind == ManifestIssueKind::Unknown)
        {
            ManifestCoherence::Unknown
        } else if issues
            .iter()
            .any(|issue| issue.kind == ManifestIssueKind::Degraded)
        {
            ManifestCoherence::Degraded
        } else {
            ManifestCoherence::Coherent
        };

        Ok(ManifestVerification {
            coherence,
            computed_manifest_sha256,
            issues,
        })
    }
}

fn assess_authority_fact(
    authority_id: &str,
    fact: &AuthorityFact,
    issues: &mut Vec<ManifestIssue>,
) {
    match fact.status {
        AuthorityStatus::Available => {}
        AuthorityStatus::Degraded => push_issue(
            issues,
            ManifestIssueKind::Degraded,
            Some(authority_id),
            "authority reports degraded status",
        ),
        AuthorityStatus::Unavailable | AuthorityStatus::Unknown => push_issue(
            issues,
            ManifestIssueKind::Unknown,
            Some(authority_id),
            "authority is unavailable or unknown",
        ),
        AuthorityStatus::Drift => push_issue(
            issues,
            ManifestIssueKind::Drift,
            Some(authority_id),
            "authority reports drift",
        ),
    }

    match fact.freshness {
        AuthorityFreshness::Fresh => {}
        AuthorityFreshness::Stale => push_issue(
            issues,
            ManifestIssueKind::Degraded,
            Some(authority_id),
            "authority observation is stale",
        ),
        AuthorityFreshness::Unknown => push_issue(
            issues,
            ManifestIssueKind::Unknown,
            Some(authority_id),
            "authority freshness is unknown",
        ),
    }

    if matches!(
        fact.status,
        AuthorityStatus::Unavailable | AuthorityStatus::Unknown
    ) {
        if !fact.revision.is_empty() || !fact.digest.is_empty() {
            push_issue(
                issues,
                ManifestIssueKind::Unknown,
                Some(authority_id),
                "unavailable authority carried an untrusted prior value",
            );
        }
        return;
    }

    if fact.revision.is_empty() || fact.digest.is_empty() || fact.observed_at == 0 {
        push_issue(
            issues,
            ManifestIssueKind::Drift,
            Some(authority_id),
            "available/degraded authority omitted revision, digest, or observed_at",
        );
    }
}

fn push_issue(
    issues: &mut Vec<ManifestIssue>,
    kind: ManifestIssueKind,
    authority_id: Option<&str>,
    detail: impl Into<String>,
) {
    issues.push(ManifestIssue {
        kind,
        authority_id: authority_id.map(str::to_owned),
        detail: detail.into(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;

    const SOURCE_DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const BINARY_DIGEST: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const GRAPH_DIGEST: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    const SKELETON_DIGEST: &str =
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
    const UI_DIGEST: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    const RELEASE_DIGEST: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

    fn manifest() -> OrganismManifestV1 {
        let version = "1.0.0";
        let mut authorities = BTreeMap::new();
        authorities.insert(
            SOURCE_AUTHORITY_ID.into(),
            AuthorityFact::available(version, SOURCE_DIGEST, 100),
        );
        authorities.insert(
            RUNTIME_BINARY_AUTHORITY_ID.into(),
            AuthorityFact::available(SOURCE_DIGEST, BINARY_DIGEST, 100),
        );
        authorities.insert(
            GRAPH_AUTHORITY_ID.into(),
            AuthorityFact::available("7", GRAPH_DIGEST, 100),
        );
        authorities.insert(
            ARCHITECTURE_AUTHORITY_ID.into(),
            AuthorityFact::available("9", SKELETON_DIGEST, 100),
        );
        authorities.insert(
            UI_BUNDLE_AUTHORITY_ID.into(),
            AuthorityFact::available(version, UI_DIGEST, 100),
        );
        authorities.insert(
            RELEASE_AUTHORITY_ID.into(),
            AuthorityFact::available(version, RELEASE_DIGEST, 100),
        );

        let mut supported_modes = BTreeSet::new();
        supported_modes.insert("HUMAN_GATED".into());
        let mechanically_proven_modes = supported_modes.clone();

        let mut manifest = OrganismManifestV1 {
            schema: ORGANISM_MANIFEST_SCHEMA.into(),
            organism_id: "organism:one".into(),
            repo_id: "repo:one".into(),
            brain_id: "brain:one".into(),
            project_root_fingerprint: "root:fingerprint".into(),
            source: SourceFact {
                commit: SOURCE_DIGEST.into(),
                dirty: false,
                version: version.into(),
            },
            runtime: RuntimeFact {
                owner_id: "owner:one".into(),
                binary_version: version.into(),
                binary_sha256: BINARY_DIGEST.into(),
                started_at: 90,
            },
            graph: GraphFact {
                generation: 7,
                snapshot_sha256: GRAPH_DIGEST.into(),
                node_count: 10,
                edge_count: 20,
            },
            architecture: ArchitectureFact {
                store_version: 9,
                skeleton_digest: SKELETON_DIGEST.into(),
                ratification_state: "RATIFIED".into(),
            },
            ui: UiFact {
                bundle_version: version.into(),
                bundle_sha256: UI_DIGEST.into(),
                mode: "PRODUCTION".into(),
            },
            capabilities: CapabilitiesFact {
                policy_version: "policy-v1".into(),
                enabled_effects: BTreeSet::new(),
            },
            autonomy: AutonomyFact {
                supported_modes,
                mechanically_proven_modes,
                active_mode: "HUMAN_GATED".into(),
                activation_receipt_id: "activation:bootstrap".into(),
                constitution_digest: "constitution:digest".into(),
                constitution_epoch: 1,
                safety_kernel_digest: "safety:digest".into(),
                autonomy_epoch: 1,
                grants_digest: "grants:digest".into(),
                quorum_policy_digest: "quorum:digest".into(),
                max_effective_tier_projection: "A0".into(),
                issuance_frozen: false,
                sentinel_safety_state: "GREEN".into(),
            },
            schemas: SchemasFact {
                mission: "mission-v1".into(),
                receipt: "receipt-v1".into(),
                checkpoint: "checkpoint-v1".into(),
                light: "light-v1".into(),
                system_blocks: "system-blocks-v1".into(),
            },
            authorities,
            release_provenance: ReleaseProvenanceFact {
                release_candidate_digest: RELEASE_DIGEST.into(),
                signature: OpaqueSignature::new("opaque-release-signature"),
            },
            generated_at: 100,
            manifest_sha256: String::new(),
        };
        manifest.seal().unwrap();
        manifest
    }

    #[test]
    fn self_hash_omits_only_its_own_field() {
        let mut manifest = manifest();
        let expected = manifest.compute_manifest_sha256().unwrap();
        manifest.manifest_sha256 = "different-placeholder".into();
        assert_eq!(manifest.compute_manifest_sha256().unwrap(), expected);

        manifest.graph.node_count += 1;
        assert_ne!(manifest.compute_manifest_sha256().unwrap(), expected);
    }

    #[test]
    fn sealed_manifest_verifies_coherent() {
        let report = manifest().verify().unwrap();
        assert!(report.is_coherent(), "issues: {:?}", report.issues);
    }

    #[test]
    fn wire_shape_matches_prd_6_1_exact_fields() {
        let value = serde_json::to_value(manifest()).unwrap();
        let actual: BTreeSet<String> = value.as_object().unwrap().keys().cloned().collect();
        let expected: BTreeSet<String> = [
            "schema",
            "organism_id",
            "repo_id",
            "brain_id",
            "project_root_fingerprint",
            "source",
            "runtime",
            "graph",
            "architecture",
            "ui",
            "capabilities",
            "autonomy",
            "schemas",
            "authorities",
            "release_provenance",
            "generated_at",
            "manifest_sha256",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        assert_eq!(actual, expected);

        let source: BTreeSet<String> = value["source"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(
            source,
            ["commit", "dirty", "version"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );

        let authority: BTreeSet<String> = value["authorities"][SOURCE_AUTHORITY_ID]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(
            authority,
            ["revision", "digest", "observed_at", "freshness", "status"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
    }

    #[test]
    fn changed_manifest_byte_is_drift() {
        let mut manifest = manifest();
        manifest.graph.node_count += 1;
        let report = manifest.verify().unwrap();
        assert_eq!(report.coherence, ManifestCoherence::Drift);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.detail.contains("manifest_sha256")));
    }

    #[test]
    fn unavailable_authority_is_unknown_not_fresh_old_data() {
        let mut manifest = manifest();
        manifest.authorities.insert(
            RUNTIME_BINARY_AUTHORITY_ID.into(),
            AuthorityFact::unavailable(110),
        );
        manifest.seal().unwrap();

        let report = manifest.verify().unwrap();
        assert_eq!(report.coherence, ManifestCoherence::Unknown);
        let fact = &manifest.authorities[RUNTIME_BINARY_AUTHORITY_ID];
        assert!(fact.revision.is_empty());
        assert!(fact.digest.is_empty());
    }

    #[test]
    fn stale_authority_is_degraded() {
        let mut manifest = manifest();
        let fact = manifest.authorities.get_mut(GRAPH_AUTHORITY_ID).unwrap();
        fact.status = AuthorityStatus::Degraded;
        fact.freshness = AuthorityFreshness::Stale;
        manifest.seal().unwrap();

        assert_eq!(
            manifest.verify().unwrap().coherence,
            ManifestCoherence::Degraded
        );
    }

    #[test]
    fn source_binary_and_bundle_projection_divergence_are_drift() {
        let mut cases = Vec::new();

        let mut source = manifest();
        source.source.commit = "changed-source".into();
        source.seal().unwrap();
        cases.push(source);

        let mut binary = manifest();
        binary.runtime.binary_sha256 = "changed-binary".into();
        binary.seal().unwrap();
        cases.push(binary);

        let mut bundle = manifest();
        bundle.ui.bundle_sha256 = "changed-bundle".into();
        bundle.seal().unwrap();
        cases.push(bundle);

        for case in cases {
            assert_eq!(case.verify().unwrap().coherence, ManifestCoherence::Drift);
        }
    }

    #[test]
    fn source_binary_bundle_version_divergence_is_drift() {
        let mut manifest = manifest();
        manifest.runtime.binary_version = "2.0.0".into();
        manifest
            .authorities
            .get_mut(RUNTIME_BINARY_AUTHORITY_ID)
            .unwrap()
            .revision = "2.0.0".into();
        manifest.seal().unwrap();

        assert_eq!(
            manifest.verify().unwrap().coherence,
            ManifestCoherence::Drift
        );
    }

    #[test]
    fn available_authority_without_digest_is_drift() {
        let mut manifest = manifest();
        manifest
            .authorities
            .get_mut(GRAPH_AUTHORITY_ID)
            .unwrap()
            .digest
            .clear();
        manifest.seal().unwrap();
        assert_eq!(
            manifest.verify().unwrap().coherence,
            ManifestCoherence::Drift
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let mut value = serde_json::to_value(manifest()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("surprise".into(), json!(true));
        assert!(serde_json::from_value::<OrganismManifestV1>(value).is_err());
    }

    #[test]
    fn authority_fact_unknown_fields_are_rejected() {
        let value = json!({
            "revision": "1",
            "digest": "digest",
            "observed_at": 1,
            "freshness": "FRESH",
            "status": "AVAILABLE",
            "invented": true
        });
        assert!(serde_json::from_value::<AuthorityFact>(value).is_err());
    }
}
