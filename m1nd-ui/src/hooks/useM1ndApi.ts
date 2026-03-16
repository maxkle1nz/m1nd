import { useCallback, useRef, useState } from 'react';
import { api, ApiError } from '../api/client';
import type { ToolId } from '../types';
import { useGraphStore } from '../stores/graphStore';

export interface ApiCallState {
  loading: boolean;
  error: string | null;
}

/**
 * React hook wrapping the m1nd API client with loading/error state
 * and automatic graph store updates.
 */
export function useM1ndApi() {
  const [state, setState] = useState<ApiCallState>({ loading: false, error: null });
  const abortRef = useRef<AbortController | null>(null);

  const { setLoading, setError, loadSubgraph } = useGraphStore();

  const runQuery = useCallback(async (tool: ToolId, params: Record<string, unknown>) => {
    // Cancel previous in-flight request
    abortRef.current?.abort();
    abortRef.current = new AbortController();

    setState({ loading: true, error: null });
    setLoading(true);
    setError(null);

    try {
      // For tools that produce subgraphs, use the subgraph endpoint
      const subgraphTools: ToolId[] = ['activate', 'seek', 'impact', 'missing', 'differential'];

      if (subgraphTools.includes(tool)) {
        const query = (params.query as string) || (params.node_id as string) || '';
        const result = await api.subgraph(query, (params.top_k as number) ?? 30);
        loadSubgraph(result.nodes, result.edges, query, tool);
      } else {
        // For other tools, call directly and show result in detail panel
        await api.callTool(tool, params);
        setLoading(false);
      }

      setState({ loading: false, error: null });
    } catch (err) {
      const msg = err instanceof ApiError
        ? `${err.errorType}: ${err.detail}`
        : err instanceof Error
          ? err.message
          : 'Unknown error';
      setState({ loading: false, error: msg });
      setLoading(false);
      setError(msg);
    }
  }, [setLoading, setError, loadSubgraph]);

  const fetchHealth = useCallback(() => api.health(), []);
  const fetchStats = useCallback(() => api.graphStats(), []);

  return { ...state, runQuery, fetchHealth, fetchStats };
}
