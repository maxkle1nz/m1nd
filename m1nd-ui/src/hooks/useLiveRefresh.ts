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
  graphChangeConcernsBrain,
  GRAPH_CHANGED_DEBOUNCE_MS,
  FALLBACK_POLL_MS,
} from './liveRefreshCore';

export {
  createGraphChangeDebouncer,
  isGraphChanged,
  graphChangeConcernsBrain,
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
  /**
   * The brain the surface is viewing (§4A.9.6): `null` = the bound graph. A
   * `graph_changed` event refetches only when it names THIS brain or carries no
   * brain (the over-refetch on old owners). The fallback stats poll rides the
   * same selector so it watches the VIEWED brain's counts, not the bound graph's.
   */
  viewedRoot?: string | null;
}

export function useLiveRefresh({
  onRefresh,
  enabled = true,
  debounceMs = GRAPH_CHANGED_DEBOUNCE_MS,
  viewedRoot = null,
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
  // The viewed brain, in a ref so the stable SSE callback sees the latest root
  // and the fallback poll watches the right brain's counts.
  const viewedRootRef = useRef(viewedRoot);
  viewedRootRef.current = viewedRoot;
  // A brain switch invalidates the last-seen counts (different brain, different
  // size — never mistake B's size for A's "change").
  useEffect(() => {
    lastStats.current = null;
  }, [viewedRoot]);

  // ── Primary path: SSE graph_changed ─────────────────────────────────────────
  const onEvent = useCallback((event: SseEvent) => {
    if (event.event_type !== 'graph_changed') return;
    // Any graph_changed proves the live wire works (stop the fallback poll), even
    // one for another brain — but only refetch when it concerns OUR brain.
    sseAlive.current = true;
    if (graphChangeConcernsBrain(event, viewedRootRef.current)) {
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
        const stats = await api.graphStats(viewedRootRef.current);
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
