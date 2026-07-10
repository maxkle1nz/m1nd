import type {
  HealthResponse,
  InstanceListResponse,
  InstanceSelfResponse,
  SubgraphResponse,
  ToolCallResult,
  ToolsResponse,
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

// The base is ALWAYS same-origin ('') so requests ride the Vite dev proxy in dev
// (which forwards /api to the owner — default :1337, retargetable via M1ND_API in
// vite.config) and the owner directly in production. Same-origin avoids the CORS
// wall a direct cross-origin owner URL hit (the browser has no allow-origin from
// the loopback owner). `VITE_M1ND_API` still lets a browser bypass the proxy to a
// CORS-enabled owner if ever needed. In a plain Node test runner import.meta.env
// is undefined, so guard the read (tests stub api.* and never hit the network).
const BASE_URL = (import.meta.env?.VITE_M1ND_API as string | undefined) ?? '';

async function apiFetch<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE_URL}${path}`, {
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
  instanceSelf: () => apiFetch<InstanceSelfResponse>('/api/instance/self'),
  instances: () => apiFetch<InstanceListResponse>('/api/instances'),
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
  missionHeads: (brain?: string | null) =>
    apiFetch<MissionsResponse>(withBrain('/api/mailbox?kind=mission', brain)),

  /**
   * The tray's compose write (HUMAN-VIEW-V2 F2.5 §2c) — `mission_post` appends one
   * mission letter to the bound brain's box after the §1 contract gates pass (schema
   * + phase gating incl. the §1d landed-law + the §1e head CAS). A stale head returns
   * `stale_head` and nothing is appended; an identical replay dedups. WRITE verb —
   * refused under a read-only attach. Bare tool route, agent_id 'gui', unwrapping the
   * `{result}` envelope like the reconcile write. `brain` scopes it to a hosted brain.
   */
  missionPost: (letter: MissionLetter, brain?: string | null) =>
    apiFetch<{ result: PostOutcome }>(withBrain('/api/tools/mission_post', brain), {
      method: 'POST',
      body: JSON.stringify({ agent_id: 'gui', letter }),
    }).then((r) => r.result),

  /**
   * The runner-daemon liveness read (HUMAN-VIEW-V2 F2.5c §5a) — `GET /api/runnerd/status`
   * lists every announced runner (`runner_id`, port, last_seen). A pure read (no
   * secret): the compose panel uses it to un-disable the spawn radio and list the
   * pinned-live runners. Empty `runners` = no daemon connected. NOT `?brain=`-scoped
   * (the registry is owner-process-global liveness, not per-brain).
   */
  runnerdStatus: () => apiFetch<RunnerdStatus>('/api/runnerd/status'),

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
  ) =>
    apiFetch<{ result: SkeletonCandidateResult }>(withBrain('/api/tools/skeleton_candidate', brain), {
      method: 'POST',
      body: JSON.stringify({
        agent_id: 'gui',
        expected_store_version: input.expectedStoreVersion,
        ...(input.reviewLimit != null ? { review_limit: input.reviewLimit } : {}),
        naming: input.naming ?? 'auto',
      }),
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
