import type {
  HealthResponse,
  InstanceListResponse,
  InstanceSelfResponse,
  PresenceResponse,
  SubgraphResponse,
  ToolCallResult,
  ToolsResponse,
  OrganismManifestResponseV1,
  AuthorityFreshness,
  AuthorityStatus,
  ManifestCoherence,
  ManifestIssueKind,
} from './types';
import type { GraphSnapshot } from '../lib/snapshot';
import type { MailboxResponse } from '../lib/mailbox';
import type {
  Receipt,
  ReconcileReport,
  RatifyResult,
  SkeletonCandidateResult,
  SystemBlocksSnapshot,
} from '../lib/buildMap';
import type {
  CandidateEditResult,
  CandidateLeaseResult,
  CandidateNamingResult,
  CurationSpawnResult,
  EditOpInput,
} from '../lib/candidateEdit';
import type {
  MissionLetter,
  MissionsResponse,
  PostOutcome,
  ReceiptImportOutcome,
  RunnerdStatus,
  SpawnInput,
  SpawnOutcome,
} from '../lib/missions';
import type { UniverseResponse } from '../lib/universe';
import type { AlertsListResponse, AlertsAckResponse } from '../lib/alerts';

// The owner-launched browser receives a one-shot bootstrap nonce in the query
// string. The first document response has already exchanged it for an HttpOnly
// cookie, so remove it before any navigation, copy, referrer, or history use.
if (typeof window !== 'undefined' && typeof window.history?.replaceState === 'function') {
  const bootstrapUrl = new URL(window.location.href);
  if (bootstrapUrl.searchParams.has('m1nd-bootstrap')) {
    bootstrapUrl.searchParams.delete('m1nd-bootstrap');
    window.history.replaceState(
      window.history.state,
      '',
      `${bootstrapUrl.pathname}${bootstrapUrl.search}${bootstrapUrl.hash}`,
    );
  }
}

// The base is ALWAYS same-origin ('') so requests ride the Vite dev proxy in dev
// (which forwards /api to the owner — default :1337, retargetable via M1ND_API in
// vite.config) and the owner directly in production. Same-origin avoids the CORS
// wall a direct cross-origin owner URL hit (the browser has no allow-origin from
// the loopback owner). `VITE_M1ND_API` still lets a browser bypass the proxy to a
// CORS-enabled owner if ever needed. In a plain Node test runner import.meta.env
// is undefined, so guard the read (tests stub api.* and never hit the network).
// Exported as the SINGLE source of base so the SSE stream (useSSE) rides the exact
// same origin — never the hardcoded loopback that broke a retargeted `M1ND_API` dev.
export const API_BASE = (import.meta.env?.VITE_M1ND_API as string | undefined) ?? '';

async function apiFetch<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...options,
  });
  if (!res.ok) {
    const error = await res.json().catch(() => ({ error: 'network', detail: res.statusText }));
    // The owner emits two error shapes: instance routes use `detail`, the
    // universal tool_error_payload uses `message` (http_server.rs:690). The Hall
    // must render either verbatim (INV-09), so accept both — never drop the human
    // string into `undefined`.
    const detail = error.detail ?? error.message ?? res.statusText;
    throw new ApiError(res.status, error.error, detail);
  }
  return res.json();
}

export class ApiError extends Error {
  constructor(
    public status: number,
    public errorType: string,
    public detail: string,
  ) {
    super(`${errorType}: ${detail}`);
  }
}

// The manifest is an authority projection, so its reader is deliberately strict:
// malformed/missing facts make the whole read unavailable. It never fills defaults,
// coerces values, recomputes authority, or rewrites the object returned by the owner.
type JsonRecord = Record<string, unknown>;

function manifestParseError(path: string, expected: string): never {
  throw new TypeError(`invalid organism manifest at ${path}: expected ${expected}`);
}

function manifestRecord(value: unknown, path: string): JsonRecord {
  if (value == null || typeof value !== 'object' || Array.isArray(value)) {
    return manifestParseError(path, 'object');
  }
  return value as JsonRecord;
}

function manifestString(value: unknown, path: string): void {
  if (typeof value !== 'string') manifestParseError(path, 'string');
}

function manifestBoolean(value: unknown, path: string): void {
  if (typeof value !== 'boolean') manifestParseError(path, 'boolean');
}

function manifestU64(value: unknown, path: string): void {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) {
    manifestParseError(path, 'non-negative safe integer');
  }
}

function manifestLiteral<T extends string>(
  value: unknown,
  path: string,
  allowed: readonly T[],
): asserts value is T {
  if (typeof value !== 'string' || !allowed.includes(value as T)) {
    manifestParseError(path, allowed.map((item) => JSON.stringify(item)).join(' | '));
  }
}

function manifestStringArray(value: unknown, path: string): void {
  if (!Array.isArray(value)) manifestParseError(path, 'string[]');
  value.forEach((item, index) => manifestString(item, `${path}[${index}]`));
}

function parseAuthorityFact(value: unknown, path: string): void {
  const fact = manifestRecord(value, path);
  manifestString(fact.revision, `${path}.revision`);
  manifestString(fact.digest, `${path}.digest`);
  manifestU64(fact.observed_at, `${path}.observed_at`);
  manifestLiteral<AuthorityFreshness>(fact.freshness, `${path}.freshness`, [
    'FRESH',
    'STALE',
    'UNKNOWN',
  ]);
  manifestLiteral<AuthorityStatus>(fact.status, `${path}.status`, [
    'AVAILABLE',
    'DEGRADED',
    'UNAVAILABLE',
    'DRIFT',
    'UNKNOWN',
  ]);
}

const CORE_MANIFEST_AUTHORITY_IDS = [
  'source',
  'runtime_binary',
  'graph',
  'architecture',
  'ui_bundle',
  'release_candidate',
] as const;

/**
 * Parse the exact G1 response without deriving or replacing any fact. The same
 * object reference is returned after validation so the UI remains a consumer,
 * never a second manifest composer.
 */
export function parseOrganismManifestResponse(value: unknown): OrganismManifestResponseV1 {
  const response = manifestRecord(value, '$');
  manifestLiteral(response.schema, '$.schema', ['m1nd-organism-manifest-response-v1']);

  const manifest = manifestRecord(response.manifest, '$.manifest');
  manifestLiteral(manifest.schema, '$.manifest.schema', ['m1nd-organism-manifest-v1']);
  for (const field of [
    'organism_id',
    'repo_id',
    'brain_id',
    'project_root_fingerprint',
    'manifest_sha256',
  ]) {
    manifestString(manifest[field], `$.manifest.${field}`);
  }

  const source = manifestRecord(manifest.source, '$.manifest.source');
  manifestString(source.commit, '$.manifest.source.commit');
  manifestBoolean(source.dirty, '$.manifest.source.dirty');
  manifestString(source.version, '$.manifest.source.version');

  const runtime = manifestRecord(manifest.runtime, '$.manifest.runtime');
  manifestString(runtime.owner_id, '$.manifest.runtime.owner_id');
  manifestString(runtime.binary_version, '$.manifest.runtime.binary_version');
  manifestString(runtime.binary_sha256, '$.manifest.runtime.binary_sha256');
  manifestU64(runtime.started_at, '$.manifest.runtime.started_at');

  const graph = manifestRecord(manifest.graph, '$.manifest.graph');
  for (const field of ['generation', 'node_count', 'edge_count']) {
    manifestU64(graph[field], `$.manifest.graph.${field}`);
  }
  manifestString(graph.snapshot_sha256, '$.manifest.graph.snapshot_sha256');

  const architecture = manifestRecord(manifest.architecture, '$.manifest.architecture');
  manifestU64(architecture.store_version, '$.manifest.architecture.store_version');
  manifestString(architecture.skeleton_digest, '$.manifest.architecture.skeleton_digest');
  manifestString(architecture.ratification_state, '$.manifest.architecture.ratification_state');

  const ui = manifestRecord(manifest.ui, '$.manifest.ui');
  manifestString(ui.bundle_version, '$.manifest.ui.bundle_version');
  manifestString(ui.bundle_sha256, '$.manifest.ui.bundle_sha256');
  manifestString(ui.mode, '$.manifest.ui.mode');

  const capabilities = manifestRecord(manifest.capabilities, '$.manifest.capabilities');
  manifestString(capabilities.policy_version, '$.manifest.capabilities.policy_version');
  manifestStringArray(capabilities.enabled_effects, '$.manifest.capabilities.enabled_effects');

  const autonomy = manifestRecord(manifest.autonomy, '$.manifest.autonomy');
  manifestStringArray(autonomy.supported_modes, '$.manifest.autonomy.supported_modes');
  manifestStringArray(
    autonomy.mechanically_proven_modes,
    '$.manifest.autonomy.mechanically_proven_modes',
  );
  for (const field of [
    'active_mode',
    'activation_receipt_id',
    'constitution_digest',
    'safety_kernel_digest',
    'grants_digest',
    'quorum_policy_digest',
    'max_effective_tier_projection',
    'sentinel_safety_state',
  ]) {
    manifestString(autonomy[field], `$.manifest.autonomy.${field}`);
  }
  for (const field of ['constitution_epoch', 'autonomy_epoch']) {
    manifestU64(autonomy[field], `$.manifest.autonomy.${field}`);
  }
  manifestBoolean(autonomy.issuance_frozen, '$.manifest.autonomy.issuance_frozen');

  const schemas = manifestRecord(manifest.schemas, '$.manifest.schemas');
  for (const field of ['mission', 'receipt', 'checkpoint', 'light', 'system_blocks']) {
    manifestString(schemas[field], `$.manifest.schemas.${field}`);
  }

  const authorities = manifestRecord(manifest.authorities, '$.manifest.authorities');
  for (const authorityId of CORE_MANIFEST_AUTHORITY_IDS) {
    parseAuthorityFact(
      authorities[authorityId],
      `$.manifest.authorities.${authorityId}`,
    );
  }
  for (const [authorityId, fact] of Object.entries(authorities)) {
    parseAuthorityFact(fact, `$.manifest.authorities.${authorityId}`);
  }

  const release = manifestRecord(manifest.release_provenance, '$.manifest.release_provenance');
  manifestString(
    release.release_candidate_digest,
    '$.manifest.release_provenance.release_candidate_digest',
  );
  manifestString(release.signature, '$.manifest.release_provenance.signature');
  manifestU64(manifest.generated_at, '$.manifest.generated_at');

  const verification = manifestRecord(response.verification, '$.verification');
  manifestLiteral<ManifestCoherence>(verification.coherence, '$.verification.coherence', [
    'COHERENT',
    'DRIFT',
    'DEGRADED',
    'UNKNOWN',
  ]);
  manifestString(
    verification.computed_manifest_sha256,
    '$.verification.computed_manifest_sha256',
  );
  if (verification.computed_manifest_sha256 !== manifest.manifest_sha256) {
    manifestParseError(
      '$.verification.computed_manifest_sha256',
      'value equal to $.manifest.manifest_sha256',
    );
  }
  if (!Array.isArray(verification.issues)) manifestParseError('$.verification.issues', 'array');
  verification.issues.forEach((value, index) => {
    const path = `$.verification.issues[${index}]`;
    const issue = manifestRecord(value, path);
    manifestLiteral<ManifestIssueKind>(issue.kind, `${path}.kind`, [
      'DRIFT',
      'DEGRADED',
      'UNKNOWN',
    ]);
    if (issue.authority_id !== null) manifestString(issue.authority_id, `${path}.authority_id`);
    manifestString(issue.detail, `${path}.detail`);
  });

  return value as OrganismManifestResponseV1;
}

/** The `/api/file` read (HUMAN-VIEW-V2 F2 Show Code viewer). Content is capped at
 *  `max_bytes` on the owner; `truncated` says so honestly and `bytes` is the true
 *  on-disk size. Repo-relative paths only — the owner refuses absolute/escape. */
export interface FileViewResponse {
  path: string;
  content: string;
  bytes: number;
  truncated: boolean;
  max_bytes: number;
}

/**
 * Append the §4A.9 `?brain=<project_root>` selector to a path when a brain root is
 * given. Absent/empty → the path is untouched (the bound graph, byte-compatible —
 * the serde-default posture applied to a URL). URL-encodes the absolute root. When
 * the path already carries a query (`?query=…`), the selector joins with `&`.
 */
function withBrain(path: string, brain?: string | null): string {
  const root = brain?.trim();
  if (!root) return path;
  const sep = path.includes('?') ? '&' : '?';
  return `${path}${sep}brain=${encodeURIComponent(root)}`;
}

export const api = {
  health: () => apiFetch<HealthResponse>('/api/health'),

  /** G1 read-only truth projection. Parsing fails closed; no fact is defaulted. */
  manifest: (brain?: string | null, signal?: AbortSignal) =>
    apiFetch<unknown>(withBrain('/api/manifest', brain), {
      cache: 'no-store',
      ...(signal ? { signal } : {}),
    }).then(parseOrganismManifestResponse),

  /**
   * The Hall presence strip's read (ORGANISM-INSIDE-PRD P1; P1-UI-CONTRACT). A
   * pure GET — safe under a read-only attach. Absent `brain` = the OWNER-WIDE
   * roster (the Hall's control-room scope); a `brain` scopes it to that brain.
   * The roster is TTL-filtered at read (no ghosts) and collisions are derived at
   * read. A pre-P1 owner has no route (404) → the caller degrades to the honest
   * empty roster rather than an error wall (`usePresences`, vigil-fail-open).
   */
  presences: (brain?: string | null, signal?: AbortSignal) =>
    apiFetch<PresenceResponse>(withBrain('/api/presences', brain), signal ? { signal } : undefined),

  instanceSelf: () => apiFetch<InstanceSelfResponse>('/api/instance/self'),
  instances: () => apiFetch<InstanceListResponse>('/api/instances'),

  /**
   * The Universe panorama's read (HUMAN-VIEW-V2 F30, `m1nd-universe-v0`). A pure
   * GET — safe under a read-only attach. Sidecar-only on the owner: every EXISTING
   * project brain with its manifest facts (size + freshness), live presences, and
   * pending human gestures (merge_wait stamps + candidate ratifies), plus the
   * owner's own alert scope — and it never hydrates a brain. A pre-F30 owner has no
   * route (404) → the caller degrades to the honest empty panorama.
   */
  universe: (signal?: AbortSignal) =>
    apiFetch<UniverseResponse>('/api/universe', signal ? { signal } : undefined),

  /**
   * The owner's daemon-alert list (honest doors: the Landing's owner item lands on
   * the Hall's alerts panel). `alerts_list` reads the BOUND session's unacked alerts
   * — the SAME stock the Universe's `owner.alerts_pending` counts (http_server.rs
   * `universe_body`). Deliberately NOT `?brain=`-scoped: a selector would list a
   * project brain's own alerts, never the owner's. Bare tool route, agent_id 'gui',
   * unwrapping the `{result}` envelope. A pure READ — safe under a read-only attach.
   */
  alertsList: () =>
    apiFetch<{ result: AlertsListResponse }>('/api/tools/alerts_list', {
      method: 'POST',
      body: JSON.stringify({ agent_id: 'gui' }),
    }).then((r) => r.result),

  /**
   * Acknowledge one or more owner daemon alerts (`alerts_ack`) — flips `acked` on the
   * BOUND session's alerts and persists, so the Universe count and the panel agree.
   * Like `alertsList`, deliberately NOT `?brain=`-scoped (the owner's alerts, never a
   * project brain's). WRITE verb — refused under a read-only attach (the panel surfaces
   * the honest refusal). Bare tool route, agent_id 'gui', unwrapping the `{result}`.
   */
  alertsAck: (alertIds: string[]) =>
    apiFetch<{ result: AlertsAckResponse }>('/api/tools/alerts_ack', {
      method: 'POST',
      body: JSON.stringify({ agent_id: 'gui', alert_ids: alertIds }),
    }).then((r) => r.result),
  saveSelfInstanceState: () =>
    apiFetch<ToolCallResult>('/api/instance/save', {
      method: 'POST',
      body: JSON.stringify({}),
    }),
  saveInstanceState: (instanceId: string) =>
    apiFetch<ToolCallResult>(`/api/instances/${encodeURIComponent(instanceId)}/save`, {
      method: 'POST',
      body: JSON.stringify({}),
    }),
  deleteInstanceState: (instanceId: string) =>
    apiFetch<{ deleted: unknown }>(`/api/instances/${encodeURIComponent(instanceId)}/delete-state`, {
      method: 'POST',
      body: JSON.stringify({}),
    }),

  tools: () => apiFetch<ToolsResponse>('/api/tools'),

  callTool: (toolName: string, params: Record<string, unknown>, brain?: string | null) =>
    apiFetch<ToolCallResult>(withBrain(`/api/tools/m1nd.${toolName}`, brain), {
      method: 'POST',
      body: JSON.stringify({ agent_id: 'gui', ...params }),
    }),

  subgraph: (query: string, topK = 30, depth = 2, brain?: string | null) => {
    const clampedTopK = Math.min(topK, 100);
    return apiFetch<SubgraphResponse>(
      withBrain(
        `/api/graph/subgraph?query=${encodeURIComponent(query)}&top_k=${clampedTopK}&depth=${depth}`,
        brain,
      ),
    );
  },

  graphStats: (brain?: string | null) =>
    apiFetch<{ node_count: number; edge_count: number }>(withBrain('/api/graph/stats', brain)),

  /** The single source of tree structure (PRD §3.1). Typed to the live wire shape.
   *  §4A.9: `brain` routes to a hosted brain (absent = the bound graph). */
  graphSnapshot: (brain?: string | null) =>
    apiFetch<GraphSnapshot>(withBrain('/api/graph/snapshot', brain)),

  /**
   * Call a tool by its BARE name (the dispatch route strips no `m1nd.` prefix —
   * `/api/tools/{tool}` maps 1:1 to the tool id). Returns the unwrapped `result`.
   * Used by the Living Tree for trust / tremor / impact / north / seek / layers.
   * §4A.9: `brain` scopes the call to a hosted brain (absent = the bound graph),
   * so every Reading-the-Tree instrument answers from the brain being viewed.
   */
  tool: <T = unknown>(
    toolName: string,
    params: Record<string, unknown> = {},
    brain?: string | null,
  ) =>
    apiFetch<{ result: T }>(withBrain(`/api/tools/${toolName}`, brain), {
      method: 'POST',
      body: JSON.stringify({ agent_id: 'gui', ...params }),
    }).then((r) => r.result),

  /**
   * The caixinha (HUMAN-LAYER-PRD §4A.11): one brain's field-report box with
   * derived fates + honest counts + the `served_brain` echo. `brain` is the
   * project root (or the literal `medulla` for the projectless box); absent →
   * the bound brain's box. The read is scoped to THIS box only (INV-17).
   */
  mailbox: (brain?: string | null) => apiFetch<MailboxResponse>(withBrain('/api/mailbox', brain)),

  /**
   * The mission tray's read (HUMAN-VIEW-V2 F2.5 §2b) — `GET /api/mailbox?kind=mission`
   * returns the per-mission heads (the §1e hash chain) + honest superseded counts +
   * the `served_brain` echo. A pre-F2.5a owner ignores `kind` and returns the
   * field-report shape (no `missions`); the tray reads that as "needs an updated
   * owner". `brain` scopes the read to a hosted brain (absent = the bound graph).
   */
  missionHeads: (brain?: string | null, signal?: AbortSignal) =>
    apiFetch<MissionsResponse>(withBrain('/api/mailbox?kind=mission', brain), signal ? { signal } : undefined),

  /**
   * The tray's compose write (HUMAN-VIEW-V2 F2.5 §2c) — `mission_post` appends one
   * mission letter to the bound brain's box after the §1 contract gates pass (schema
   * + phase gating incl. the §1d landed-law + the §1e head CAS). A stale head returns
   * `stale_head` and nothing is appended; an identical replay dedups. WRITE verb —
   * refused under a read-only attach. Bare tool route, agent_id 'gui', unwrapping the
   * `{result}` envelope like the reconcile write. `brain` scopes it to a hosted brain.
   */
  missionPost: (letter: MissionLetter, brain?: string | null, archivedVia?: string) =>
    apiFetch<{ result: PostOutcome }>(withBrain('/api/tools/mission_post', brain), {
      method: 'POST',
      // ARCHIVE gate (F2.5e): an `archived` letter is a HUMAN gesture — this screen is the
      // owner's UI, so it stamps the `archived_via:'human-ui'` origin token the backend
      // requires (the same class as receipt_import's `imported_via`). Absent for every
      // other phase (byte-identical body), so only the archive path carries it.
      body: JSON.stringify({ agent_id: 'gui', letter, ...(archivedVia ? { archived_via: archivedVia } : {}) }),
    }).then((r) => r.result),

  /**
   * The runner-daemon liveness read (HUMAN-VIEW-V2 F2.5c §5a) — `GET /api/runnerd/status`
   * lists every announced runner (`runner_id`, port, last_seen). A pure read (no
   * secret): the compose panel uses it to un-disable the spawn radio and list the
   * pinned-live runners. Empty `runners` = no daemon connected. NOT `?brain=`-scoped
   * (the registry is owner-process-global liveness, not per-brain).
   */
  runnerdStatus: (signal?: AbortSignal) =>
    apiFetch<RunnerdStatus>('/api/runnerd/status', signal ? { signal } : undefined),

  /**
   * The compose panel's spawn write (HUMAN-VIEW-V2 F2.5c §4b) — `mission_spawn` is
   * the OWNER→runner-daemon PROXY. The browser holds no shared secret, so the spawn
   * travels through the owner: it resolves the live runner + the secret + the
   * workspace, then forwards the packet to the daemon's `/run`. Returns
   * `{mission_id, accepted}` or the daemon's honest refusal. WRITE verb — refused
   * under a read-only attach. `brain` scopes the workspace/routing to a hosted brain.
   */
  missionSpawn: (input: SpawnInput, brain?: string | null) =>
    apiFetch<{ result: SpawnOutcome }>(withBrain('/api/tools/mission_spawn', brain), {
      method: 'POST',
      body: JSON.stringify({
        agent_id: 'gui',
        runner_id: input.runnerId,
        packet_markdown: input.packetMarkdown,
        block_id: input.blockId,
        brain_ref: input.brainRef,
      }),
    }).then((r) => r.result),

  /**
   * The human landing's receipt import (HUMAN-VIEW-V2 F2.5d §6) — `receipt_import` is
   * the anti-poison WRITE that attaches the gate's evidence to a block after the OCC +
   * scope + evidence-contract gates pass, bumping `store_version`. The tray hands it the
   * candidate's scope versions (NEVER re-dated) with the fresh `expected_store_version`;
   * a `stale_scope` (the boundary moved) or a `conflict` refuses and nothing is applied.
   * WRITE verb — refused under a read-only attach. Bare tool route, agent_id 'gui',
   * unwrapping the `{result}` envelope like `missionPost`. `brain` scopes the write to
   * a hosted brain (absent = the bound graph).
   *
   * LANDING A RECEIPT IS THE HUMAN GESTURE (sovereign-stamp arc, step 0): this screen is
   * the owner's UI path, so it stamps the `imported_via:'human-ui'` origin token the
   * backend now requires — the SAME class of gate `system_blocks_ratify` carries. An
   * agent/runner MCP client never composes it, so an import without it is refused
   * `human_gesture_required`. The token is forgeable on an unauthenticated loopback, so
   * it closes the cheap reflex vector, not a same-UID process (§5d; Touch ID is step 2).
   */
  receiptImport: (
    input: { expectedStoreVersion: number; blockId: string; receipt: Receipt },
    brain?: string | null,
  ) =>
    apiFetch<{ result: ReceiptImportOutcome }>(withBrain('/api/tools/receipt_import', brain), {
      method: 'POST',
      body: JSON.stringify({
        agent_id: 'gui',
        expected_store_version: input.expectedStoreVersion,
        block_id: input.blockId,
        receipt: input.receipt,
        imported_via: 'human-ui',
      }),
    }).then((r) => r.result),

  /**
   * The Build Map's read (HUMAN-VIEW-V2 F1). The `system_blocks_snapshot` verb
   * serves the ratified SystemBlock store (or an honest `present:false`) — a pure
   * READ, safe under a read-only attach. Bare tool route (`/api/tools/{tool}`),
   * agent_id 'gui', unwrapping the `{result}` envelope, like `api.tool`. §4A.9:
   * `brain` scopes the read to a hosted brain (absent = the bound graph).
   */
  systemBlocksSnapshot: (brain?: string | null) =>
    apiFetch<{ result: SystemBlocksSnapshot }>(
      withBrain('/api/tools/system_blocks_snapshot', brain),
      { method: 'POST', body: JSON.stringify({ agent_id: 'gui' }) },
    ).then((r) => r.result),

  /**
   * The Build Map's reconcile gesture (HUMAN-VIEW-V2 F3b) — the `system_blocks_reconcile`
   * WRITE verb (Slice 3). Resolves every block's membership against the real file
   * list, bumps moved boundaries, and surfaces the real unmapped. OCC-keyed on the
   * `expected_store_version` the caller read: a stale version rejects with a
   * `conflict` (nothing applied); under a read-only attach the verb is refused. Bare
   * tool route, agent_id 'gui', unwrapping the `{result}` envelope like the snapshot
   * read. §4A.9: `brain` scopes the write to a hosted brain (absent = the bound graph).
   */
  systemBlocksReconcile: (expectedStoreVersion: number, brain?: string | null) =>
    apiFetch<{ result: ReconcileReport }>(
      withBrain('/api/tools/system_blocks_reconcile', brain),
      {
        method: 'POST',
        body: JSON.stringify({ agent_id: 'gui', expected_store_version: expectedStoreVersion }),
      },
    ).then((r) => r.result),

  /**
   * The Build Map's scan gesture (HUMAN-VIEW-V2 F0c §5) — the `skeleton_candidate`
   * WRITE verb. Scans the bound repo's graph + file list into a CANDIDATE map the
   * human ratifies (auto-clustering only ever produces a candidate — the Ratification
   * law). OCC-keyed on `expected_store_version`: `null` on the first scan (no store
   * yet); the read version on a re-scan (a candidate store is replaced wholesale with
   * zero inheritance; a ratified store receives only a side-by-side candidate_revision).
   * `naming:"auto"` tries the naming-runner then falls back to marked heuristics.
   * WRITE verb — refused under a read-only attach (the honest toast says so). Bare tool
   * route, agent_id 'gui', unwrapping the `{result}` envelope. §4A.9: `brain` scopes it.
   */
  skeletonCandidate: (
    input: { expectedStoreVersion: number | null; reviewLimit?: number; naming?: 'auto' | 'heuristic' },
    brain?: string | null,
    // The scan is one LONG synchronous POST on the owner (clustering + a naming
    // batch that can wait ~2 minutes) — the wait panel's "stop waiting" gesture
    // aborts the browser side through this signal (the owner still finishes).
    signal?: AbortSignal,
  ) =>
    apiFetch<{ result: SkeletonCandidateResult }>(withBrain('/api/tools/skeleton_candidate', brain), {
      method: 'POST',
      body: JSON.stringify({
        agent_id: 'gui',
        expected_store_version: input.expectedStoreVersion,
        ...(input.reviewLimit != null ? { review_limit: input.reviewLimit } : {}),
        naming: input.naming ?? 'auto',
      }),
      ...(signal != null ? { signal } : {}),
    }).then((r) => r.result),

  /**
   * The Review-&-ratify walk's blanket ratify (HUMAN-VIEW-V2 F0c §5) — the
   * `system_blocks_ratify` WRITE verb. Flips every candidate block `candidate ->
   * ratified` (and membership `proposed -> ratified`), stamps the skeleton's
   * ratification, and bumps `store_version`. Omit `blockIds` to ratify EVERY block
   * (the reviewed blanket gesture); `ratifier` is stamped into the record. OCC-keyed
   * on `expected_store_version` — a stale version rejects with a `conflict` and
   * NOTHING is applied. WRITE verb — refused under a read-only attach. Bare tool
   * route, agent_id 'gui', unwrapping the `{result}` envelope. §4A.9: `brain` scopes it.
   *
   * This client supplies intent and OCC data only. It does not mint authority:
   * generic ratification is refused until an exact typed G2/G3 sovereign lease
   * path is installed.
   */
  systemBlocksRatify: (
    input: { expectedStoreVersion: number; ratifier: string; blockIds?: string[] },
    brain?: string | null,
  ) =>
    apiFetch<{ result: RatifyResult }>(withBrain('/api/tools/system_blocks_ratify', brain), {
      method: 'POST',
      body: JSON.stringify({
        agent_id: 'gui',
        expected_store_version: input.expectedStoreVersion,
        ratifier: input.ratifier,
        ...(input.blockIds != null ? { block_ids: input.blockIds } : {}),
      }),
    }).then((r) => r.result),

  /**
   * Edit Names & Boundaries' gesture write (HUMAN-VIEW-V2 F11-a/§4b) — the
   * `candidate_edit` WRITE verb: ONE typed batch (rename/merge/split/move_member/
   * resolve_seam/assign_unmapped) under one OCC transaction with preflight-on-a-
   * clone: the first invalid op aborts the whole batch with its index and NOTHING
   * is applied; success persists once and bumps `store_version` once. A ratified
   * skeleton refuses every op (`skeleton_not_candidate`). `by` defaults to the
   * owner seat (the GUI) — a rename stamps `named_by:owner` and clears
   * `needs_owner_naming` (the o6 provenance the ratify gate reads). WRITE verb —
   * refused under a read-only attach. §4A.9: `brain` scopes it.
   */
  candidateEdit: (
    input: { expectedStoreVersion: number; ops: EditOpInput[]; by?: 'owner' | 'runner' },
    brain?: string | null,
  ) =>
    apiFetch<{ result: CandidateEditResult }>(withBrain('/api/tools/candidate_edit', brain), {
      method: 'POST',
      body: JSON.stringify({
        agent_id: 'gui',
        expected_store_version: input.expectedStoreVersion,
        ops: input.ops,
        ...(input.by != null ? { by: input.by } : {}),
      }),
    }).then((r) => r.result),

  /**
   * The screen's "Name with runner" write (HUMAN-VIEW-V2 F11-c §2b) — the
   * `candidate_naming` HTTP-ONLY route (like `mission_spawn`, the browser never
   * holds the shared secret; the owner builds the packets, calls the daemon's
   * /name, sanitizes, and applies through `candidate_edit` under the RUNNER seat).
   * `blockIds` absent = every block still needing a name. The result is honest:
   * partial is normal ({named, fell_back}); no live naming-runner returns
   * `refusal` and touches nothing; a stale OCC key conflicts BEFORE any runner is
   * invoked. WRITE — refused under a read-only attach. §4A.9: `brain` scopes it.
   */
  candidateNaming: (
    input: { expectedStoreVersion: number; blockIds?: string[] },
    brain?: string | null,
  ) =>
    apiFetch<{ result: CandidateNamingResult }>(
      withBrain('/api/tools/candidate_naming', brain),
      {
        method: 'POST',
        body: JSON.stringify({
          agent_id: 'gui',
          expected_store_version: input.expectedStoreVersion,
          ...(input.blockIds != null ? { block_ids: input.blockIds } : {}),
        }),
      },
    ).then((r) => r.result),

  /**
   * The candidate banner's propose-apply write (HUMAN-VIEW-V2 F12 §3) —
   * `curation_spawn` is HTTP-ONLY (like `candidate_naming`/`mission_spawn`: the
   * browser never holds the shared secret). The owner composes the block-view
   * packet, calls the announced runner daemon's `/curate`, and applies the pinned
   * hand-runner's proposal through `candidate_edit` under the RUNNER seat (o5
   * sanitizes every rename, o1 preflights on a clone) at the given OCC key, then
   * posts the summary letter. The result is honest: `applied` + `ops_count` +
   * `report` on success; `refusal` (`no_hand_runner`/`proposal_malformed`/
   * `batch_refused`) when nothing applied. A stale OCC key conflicts BEFORE any
   * runner is invoked. WRITE — refused under a read-only attach. §4A.9: `brain`
   * scopes it to the hosted brain being viewed.
   */
  curationSpawn: (input: { expectedStoreVersion: number }, brain?: string | null) =>
    apiFetch<{ result: CurationSpawnResult }>(withBrain('/api/tools/curation_spawn', brain), {
      method: 'POST',
      body: JSON.stringify({
        agent_id: 'gui',
        expected_store_version: input.expectedStoreVersion,
      }),
    }).then((r) => r.result),

  /**
   * The advisory curation lease verb (HUMAN-VIEW-V2 F11-a o4) — `candidate_lease`
   * {acquire|refresh|release}. ADVISORY by law: it never blocks the owner, never
   * bumps `store_version`, and an expired lease is reclaimable by anyone. The
   * screen reads the lease from the store snapshot (`curating_by`/`curating_until`)
   * — this verb is the agent-side gesture, surfaced for parity. WRITE — refused
   * under a read-only attach. §4A.9: `brain` scopes it.
   */
  candidateLease: (
    input: { action: 'acquire' | 'refresh' | 'release'; agentId?: string; ttlSecs?: number },
    brain?: string | null,
  ) =>
    apiFetch<{ result: CandidateLeaseResult }>(withBrain('/api/tools/candidate_lease', brain), {
      method: 'POST',
      body: JSON.stringify({
        agent_id: input.agentId ?? 'gui',
        action: input.action,
        ...(input.ttlSecs != null ? { ttl_secs: input.ttlSecs } : {}),
      }),
    }).then((r) => r.result),

  /**
   * The Show Code viewer's read (HUMAN-VIEW-V2 F2). Fetches a member file's
   * content (read-only) under the brain's workspace root — a pure GET, safe under
   * a read-only attach. The owner enforces the repo-relative law + a byte cap; an
   * absolute/escape path 400s, a missing file 404s. §4A.9: `brain` scopes the read
   * to a hosted brain (absent = the bound graph).
   */
  fileView: (path: string, brain?: string | null) =>
    apiFetch<FileViewResponse>(
      withBrain(`/api/file?path=${encodeURIComponent(path)}`, brain),
    ),
};
