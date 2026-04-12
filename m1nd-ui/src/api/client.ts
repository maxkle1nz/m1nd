import type {
  HealthResponse,
  InstanceListResponse,
  InstanceSelfResponse,
  SubgraphResponse,
  ToolCallResult,
  ToolSchema,
} from './types';

const BASE_URL = import.meta.env.DEV ? 'http://localhost:1337' : '';

async function apiFetch<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE_URL}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...options,
  });
  if (!res.ok) {
    const error = await res.json().catch(() => ({ error: 'network', detail: res.statusText }));
    throw new ApiError(res.status, error.error, error.detail);
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

  graphSnapshot: () => apiFetch<unknown>('/api/graph/snapshot'),
};
