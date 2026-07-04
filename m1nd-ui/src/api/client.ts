import type {
  HealthResponse,
  InstanceListResponse,
  InstanceSelfResponse,
  SubgraphResponse,
  ToolCallResult,
  ToolsResponse,
} from './types';
import type { GraphSnapshot } from '../lib/snapshot';

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
};
