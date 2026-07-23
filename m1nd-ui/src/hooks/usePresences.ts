/*
 * usePresences — the Hall presence strip's data nerve (ORGANISM-INSIDE-PRD P1).
 *
 * Absent `brain` = the OWNER-WIDE roster (the Hall's control-room scope — every
 * agent visible to this owner across its brains). Polls on a ~5s cadence
 * (minutes-scale liveness, verdict 1b) AND refetches on the same `graph_changed`
 * nerve the tree/Hall already ride (`useLiveRefresh`). VIGIL-FAIL-OPEN: a pre-P1
 * owner has no `/api/presences` route (404) → the strip renders its honest empty
 * state, never an error wall; only a genuine non-404 failure surfaces as an error.
 */
import { useCallback, useState } from 'react';
import { api, ApiError } from '../api/client';
import type { PresenceEntry, PresenceCollision } from '../types';
import { resolveCollisions } from '../lib/presence';
import { useLiveRefresh } from './useLiveRefresh';
import { usePoller } from './usePoller';

export interface PresenceState {
  presences: PresenceEntry[];
  collisions: PresenceCollision[];
  error: string | null;
  loaded: boolean;
}

const EMPTY: PresenceState = { presences: [], collisions: [], error: null, loaded: false };

export function usePresences(enabled: boolean, brain?: string | null): PresenceState {
  const [state, setState] = useState<PresenceState>(EMPTY);

  const refresh = useCallback(async (signal?: AbortSignal) => {
    try {
      const resp = await api.presences(brain, signal);
      if (signal?.aborted) return;
      setState({
        presences: resp.presences ?? [],
        collisions: resolveCollisions(resp),
        error: null,
        loaded: true,
      });
    } catch (err) {
      // An aborted read (teardown / superseded poll) is not a failure — bail before
      // it can flip the ambient strip into an error.
      if (signal?.aborted) return;
      // A pre-P1 owner (no route) or any transient miss degrades to an empty
      // roster — presence is ambient, it never breaks the Hall (vigil-fail-open).
      // A non-404 error is surfaced honestly but still leaves the strip usable.
      const status = err instanceof ApiError ? err.status : 0;
      setState((prev) => ({
        presences: prev.presences,
        collisions: prev.collisions,
        error: status !== 0 && status !== 404 ? (err instanceof Error ? err.message : 'presence unavailable') : null,
        loaded: true,
      }));
    }
  }, [brain]);

  // ~5s roster nerve, guarded: at most one read in flight (no stacking under a
  // stalled owner), paused while the tab is hidden, aborted on teardown.
  usePoller((signal) => refresh(signal), 5000, enabled, [brain]);

  // The same graph_changed nerve the tree/Hall ride also refetches the roster.
  // Event-driven and debounced (not the interval poll), so it stays OUTSIDE the
  // guard — a mutation must never be skipped because a poll is in flight.
  useLiveRefresh({ onRefresh: () => void refresh(), enabled });

  return state;
}
