// Re-export all types from the shared types module
export type {
  GraphNode,
  GraphEdge,
  SubgraphResponse,
  HealthResponse,
  InstanceRegistryEntry,
  InstanceSelfResponse,
  InstanceListResponse,
  AgentSession,
  ToolCallResult,
  ToolCallError,
  ToolSchema,
  ToolsResponse,
  ServedBrain,
  PresenceEntry,
  PresenceMutation,
  PresenceCollision,
  PresenceCollisionReason,
  PresenceResponse,
  SseEvent,
  ToolId,
  ToolCategory,
  M1ndNodeData,
  NodeAnimationState,
  NodeAction,
  Trail,
} from '../types';

export { TOOL_CATEGORIES } from '../types';

// ── M1ND-10 G1 organism manifest ─────────────────────────────────────────────
// These are wire facts, not UI defaults. Keep them readonly so consumers cannot
// quietly turn an unavailable authority into a persuasive-looking local value.

export type ManifestCoherence = 'COHERENT' | 'DRIFT' | 'DEGRADED' | 'UNKNOWN';
export type ManifestIssueKind = 'DRIFT' | 'DEGRADED' | 'UNKNOWN';
export type AuthorityFreshness = 'FRESH' | 'STALE' | 'UNKNOWN';
export type AuthorityStatus = 'AVAILABLE' | 'DEGRADED' | 'UNAVAILABLE' | 'DRIFT' | 'UNKNOWN';

export interface ManifestSourceFact {
  readonly commit: string;
  readonly dirty: boolean;
  readonly version: string;
}

export interface ManifestRuntimeFact {
  readonly owner_id: string;
  readonly binary_version: string;
  readonly binary_sha256: string;
  readonly started_at: number;
}

export interface ManifestGraphFact {
  readonly generation: number;
  readonly snapshot_sha256: string;
  readonly node_count: number;
  readonly edge_count: number;
}

export interface ManifestArchitectureFact {
  readonly store_version: number;
  readonly skeleton_digest: string;
  readonly ratification_state: string;
}

export interface ManifestUiFact {
  readonly bundle_version: string;
  readonly bundle_sha256: string;
  readonly mode: string;
}

export interface ManifestCapabilitiesFact {
  readonly policy_version: string;
  readonly enabled_effects: readonly string[];
}

export interface ManifestAutonomyFact {
  readonly supported_modes: readonly string[];
  readonly mechanically_proven_modes: readonly string[];
  readonly active_mode: string;
  readonly activation_receipt_id: string;
  readonly constitution_digest: string;
  readonly constitution_epoch: number;
  readonly safety_kernel_digest: string;
  readonly autonomy_epoch: number;
  readonly grants_digest: string;
  readonly quorum_policy_digest: string;
  readonly max_effective_tier_projection: string;
  readonly issuance_frozen: boolean;
  readonly sentinel_safety_state: string;
}

export interface ManifestSchemasFact {
  readonly mission: string;
  readonly receipt: string;
  readonly checkpoint: string;
  readonly light: string;
  readonly system_blocks: string;
}

export interface ManifestAuthorityFact {
  readonly revision: string;
  readonly digest: string;
  readonly observed_at: number;
  readonly freshness: AuthorityFreshness;
  readonly status: AuthorityStatus;
}

export interface ManifestReleaseProvenanceFact {
  readonly release_candidate_digest: string;
  /** Opaque in G1. Presence is not signature verification. */
  readonly signature: string;
}

export interface OrganismManifestV1 {
  readonly schema: 'm1nd-organism-manifest-v1';
  readonly organism_id: string;
  readonly repo_id: string;
  readonly brain_id: string;
  readonly project_root_fingerprint: string;
  readonly source: ManifestSourceFact;
  readonly runtime: ManifestRuntimeFact;
  readonly graph: ManifestGraphFact;
  readonly architecture: ManifestArchitectureFact;
  readonly ui: ManifestUiFact;
  readonly capabilities: ManifestCapabilitiesFact;
  readonly autonomy: ManifestAutonomyFact;
  readonly schemas: ManifestSchemasFact;
  readonly authorities: Readonly<Record<string, ManifestAuthorityFact>>;
  readonly release_provenance: ManifestReleaseProvenanceFact;
  readonly generated_at: number;
  readonly manifest_sha256: string;
}

export interface ManifestIssue {
  readonly kind: ManifestIssueKind;
  readonly authority_id: string | null;
  readonly detail: string;
}

export interface ManifestVerification {
  readonly coherence: ManifestCoherence;
  readonly computed_manifest_sha256: string;
  readonly issues: readonly ManifestIssue[];
}

export interface OrganismManifestResponseV1 {
  readonly schema: 'm1nd-organism-manifest-response-v1';
  readonly manifest: OrganismManifestV1;
  readonly verification: ManifestVerification;
}
