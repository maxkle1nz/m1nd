/*
 * liveRefreshCore — the pure heart of the Living Tree's live refresh (§5.3).
 *
 * Framework-free and env-free (no React, no import.meta, no DOM) so it can be
 * unit-tested deterministically with fake timers. The React shell that wires it
 * to the SSE stream lives in useLiveRefresh.ts.
 */
import type { SseEvent } from '../types';

/** Burst debounce: an ingest emits many graph_changed events; collapse to one. */
export const GRAPH_CHANGED_DEBOUNCE_MS = 500;
/** Fallback poll cadence when SSE is unavailable (calm, not a hammer). */
export const FALLBACK_POLL_MS = 8000;

/**
 * A burst debouncer: `fire()` (re)arms the timer, so a rapid burst of calls
 * results in exactly ONE `onRefresh` after `windowMs` of quiet. `cancel()` is
 * for teardown; `pending` reports whether a refresh is queued.
 */
export function createGraphChangeDebouncer(
  onRefresh: () => void,
  windowMs = GRAPH_CHANGED_DEBOUNCE_MS,
) {
  let timer: ReturnType<typeof setTimeout> | null = null;
  return {
    fire() {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        timer = null;
        onRefresh();
      }, windowMs);
    },
    cancel() {
      if (timer) clearTimeout(timer);
      timer = null;
    },
    get pending() {
      return timer !== null;
    },
  };
}

/** True only for the `graph_changed` class — the one event that means "refetch". */
export function isGraphChanged(event: SseEvent): boolean {
  return event.event_type === 'graph_changed';
}
