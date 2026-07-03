/*
 * useLiveRefresh — the Living Tree goes live (HUMAN-LAYER-PRD §5.3).
 *
 * When an agent mutates the shared graph, the server emits a `graph_changed` SSE
 * event (http_server.rs `browser_graph_changed_event`). This hook debounces a
 * burst of those events (~500 ms — an `ingest` fans out many mutations) and then
 * calls `onRefresh` ONCE, so the tree re-fetches the snapshot and updates rows in
 * place. Refresh is CALM by construction: it re-renders touched rows, no flash,
 * no glow (the SOFT PROOF "nothing glows" rule, §6.3).
 *
 * Graceful degradation (§5.3): if SSE never connects (or drops), a low-frequency
 * poll of `/api/graph/stats` watches the node/edge counts and triggers the same
 * `onRefresh` when they change — the tree is never silently stale, and it never
 * hammers the server.
 */
import { useCallback, useEffect, useRef } from 'react';
import { useSSE } from './useSSE';
import { api } from '../api/client';
import type { SseEvent } from '../types';
import {
  createGraphChangeDebouncer,
  isGraphChanged,
  GRAPH_CHANGED_DEBOUNCE_MS,
  FALLBACK_POLL_MS,
} from './liveRefreshCore';

export {
  createGraphChangeDebouncer,
  isGraphChanged,
  GRAPH_CHANGED_DEBOUNCE_MS,
  FALLBACK_POLL_MS,
} from './liveRefreshCore';

interface UseLiveRefreshOptions {
  /** Called (debounced) when the shared graph changed and the tree should reload. */
  onRefresh: () => void;
  /** Gate the whole subscription (e.g. only when the backend is up). */
  enabled?: boolean;
  /** Override the debounce window (tests). */
  debounceMs?: number;
}

export function useLiveRefresh({
  onRefresh,
  enabled = true,
  debounceMs = GRAPH_CHANGED_DEBOUNCE_MS,
}: UseLiveRefreshOptions) {
  // Keep the latest onRefresh without re-subscribing SSE on every render.
  const refreshRef = useRef(onRefresh);
  refreshRef.current = onRefresh;

  // One debouncer instance for the component's lifetime.
  const debouncerRef = useRef<ReturnType<typeof createGraphChangeDebouncer> | null>(null);
  if (!debouncerRef.current) {
    debouncerRef.current = createGraphChangeDebouncer(() => refreshRef.current(), debounceMs);
  }

  // Did SSE ever deliver an event? If so, we trust it and stop the fallback poll.
  const sseAlive = useRef(false);
  // Last observed graph size (fallback-poll change detector).
  const lastStats = useRef<{ node_count: number; edge_count: number } | null>(null);

  // ── Primary path: SSE graph_changed ─────────────────────────────────────────
  const onEvent = useCallback((event: SseEvent) => {
    if (isGraphChanged(event)) {
      sseAlive.current = true;
      debouncerRef.current?.fire();
    }
  }, []);
  useSSE({ enabled, onEvent });

  // ── Fallback path: poll /api/graph/stats until SSE proves itself ─────────────
  useEffect(() => {
    if (!enabled) return;
    let mounted = true;
    const id = setInterval(async () => {
      // Once SSE has delivered even one event, the live path works — don't poll.
      if (sseAlive.current) return;
      try {
        const stats = await api.graphStats();
        if (!mounted) return;
        const prev = lastStats.current;
        lastStats.current = stats;
        if (
          prev &&
          (prev.node_count !== stats.node_count || prev.edge_count !== stats.edge_count)
        ) {
          debouncerRef.current?.fire();
        }
      } catch {
        // A failing stats poll is non-fatal; keep trying quietly.
      }
    }, FALLBACK_POLL_MS);
    return () => {
      mounted = false;
      clearInterval(id);
    };
  }, [enabled]);

  useEffect(() => () => debouncerRef.current?.cancel(), []);
}
