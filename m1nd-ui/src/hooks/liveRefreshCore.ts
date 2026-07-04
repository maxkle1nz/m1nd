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

/**
 * Should THIS `graph_changed` event refetch the tree viewing `viewedRoot`
 * (HUMAN-LAYER-PRD §4A.9.6)? Not every mutation touches the brain on screen.
 *
 * - The event carries no `brain_root` (a bound/legacy mutation, or a pre-2H owner
 *   that never stamps it) → refetch: the honest over-refetch, never silently
 *   stale.
 * - The event names a brain → refetch ONLY when it is the one being viewed.
 *   `viewedRoot == null` is the bound brain; a bound-scoped event carries no
 *   `brain_root`, so a hosted-brain mutation does NOT disturb the bound tree.
 *
 * Comparison is exact on the string the server emitted (it stamps its own
 * canonical root); the client passes its viewed root through unchanged.
 */
export function graphChangeConcernsBrain(event: SseEvent, viewedRoot: string | null): boolean {
  if (event.event_type !== 'graph_changed') return false;
  const eventRoot = event.data.brain_root;
  if (eventRoot == null || eventRoot === '') return true; // unscoped → over-refetch
  return eventRoot === viewedRoot; // scoped → only my brain
}
