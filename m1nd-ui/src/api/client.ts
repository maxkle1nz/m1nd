import type {
  HealthResponse,
  InstanceListResponse,
  InstanceSelfResponse,
  SubgraphResponse,
  ToolCallResult,
  ToolSchema,
} from './types';
import type { GraphSnapshot } from '../lib/snapshot';

// In Vite, import.meta.env.DEV points the browser at the dev server; in a plain
// Node test runner import.meta.env is undefined, so guard the read (the tests
// never hit the network — they stub api.* — but the module must import cleanly).
const BASE_URL = import.meta.env?.DEV ? 'http://localhost:1337' : '';

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

  tools: () => apiFetch<{ tools: ToolSchema[] }>('/api/tools'),

  callTool: (toolName: string, params: Record<string, unknown>) =>
    apiFetch<ToolCallResult>(`/api/tools/m1nd.${toolName}`, {
      method: 'POST',
      body: JSON.stringify({ agent_id: 'gui', ...params }),
    }),

  subgraph: (query: string, topK = 30, depth = 2) => {
    const clampedTopK = Math.min(topK, 100);
    return apiFetch<SubgraphResponse>(
      `/api/graph/subgraph?query=${encodeURIComponent(query)}&top_k=${clampedTopK}&depth=${depth}`,
    );
  },

  graphStats: () => apiFetch<{ node_count: number; edge_count: number }>(
    '/api/graph/stats',
  ),

  /** The single source of tree structure (PRD §3.1). Typed to the live wire shape. */
  graphSnapshot: () => apiFetch<GraphSnapshot>('/api/graph/snapshot'),

  /**
   * Call a tool by its BARE name (the dispatch route strips no `m1nd.` prefix —
   * `/api/tools/{tool}` maps 1:1 to the tool id). Returns the unwrapped `result`.
   * Used by the Living Tree for trust / tremor / impact / north.
   */
  tool: <T = unknown>(toolName: string, params: Record<string, unknown> = {}) =>
    apiFetch<{ result: T }>(`/api/tools/${toolName}`, {
      method: 'POST',
      body: JSON.stringify({ agent_id: 'gui', ...params }),
    }).then((r) => r.result),
};
