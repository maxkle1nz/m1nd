/*
 * useRunnerdStatus — the compose panel's runner-daemon liveness read (HUMAN-VIEW-V2
 * F2.5c §5a). Polls `GET /api/runnerd/status` and hands the compose panel the live
 * runners so the `spawn` radio un-disables (§4b) and its runner select lists the
 * pinned-live ids. Read-only: the hook never writes (the only spawn write is the
 * compose flow's `mission_spawn`, via PacketCompose).
 *
 * The status registry is owner-process-global LIVENESS, not per-brain, so this read
 * carries NO `?brain=` selector (unlike the mission-heads read). Poll cadence: one
 * fetch on mount, then an ~8s interval ONLY while `enabled` (the compose panel is
 * open) — a closed panel needs no runner poll. The fetch heart is extracted as
 * `loadRunnerdStatus` so the honest-degrade branch is provable DOM-free.
 */
import { useCallback, useState } from 'react';
import { api } from '../api/client';
import { runnerdAvailable, type LiveRunner, type RunnerdStatus } from '../lib/missions';
import { usePoller } from './usePoller';

/** loading → the first fetch is in flight · ready → runners served (possibly empty) ·
 *  unsupported → the owner has no `/api/runnerd/status` route (a pre-F2.5c owner) ·
 *  error → the read failed (network). */
export type RunnerdStatusState = 'loading' | 'ready' | 'unsupported' | 'error';

export interface RunnerdStatusData {
  runners: LiveRunner[];
  available: boolean;
  state: RunnerdStatusState;
  reload: () => void;
}

/** The sinks `loadRunnerdStatus` writes through — injectable so the fetch heart is
 *  testable without a DOM. */
export interface RunnerdStatusSinks {
  isMounted: () => boolean;
  setRunners: (r: LiveRunner[]) => void;
  setState: (s: RunnerdStatusState) => void;
}

/**
 * The fetch heart. A response WITHOUT a `runners` array means the owner has no
 * runnerd surface (a pre-F2.5c owner) — that is `unsupported` (spawn stays disabled
 * with the honest note), never a fake "no runners". A thrown fetch is `error` (also
 * spawn-disabled). A well-formed response with an empty `runners` is `ready` — the
 * honest "no runner daemon connected".
 */
export async function loadRunnerdStatus(
  sinks: RunnerdStatusSinks,
  signal?: AbortSignal,
): Promise<void> {
  try {
    const resp: RunnerdStatus = await api.runnerdStatus(signal);
    if (!sinks.isMounted()) return;
    if (!Array.isArray(resp.runners)) {
      sinks.setState('unsupported');
      return;
    }
    sinks.setRunners(resp.runners);
    sinks.setState('ready');
  } catch {
    if (!sinks.isMounted()) return;
    sinks.setState('error');
  }
}

export function useRunnerdStatus(enabled: boolean): RunnerdStatusData {
  const [runners, setRunners] = useState<LiveRunner[]>([]);
  const [state, setState] = useState<RunnerdStatusState>('loading');
  const [tick, setTick] = useState(0);
  const reload = useCallback(() => setTick((t) => t + 1), []);

  // ~8s liveness poll, guarded: at most one status read in flight (no stacking
  // under a stalled owner), paused while the tab is hidden, aborted on teardown.
  // The fetch heart's `isMounted` is bridged to the poll's AbortSignal so a torn-
  // down or superseded read never writes state (and never flips to a false error).
  usePoller(
    (signal) =>
      loadRunnerdStatus({ isMounted: () => !signal.aborted, setRunners, setState }, signal),
    8000,
    enabled,
    [tick],
  );

  return {
    runners,
    available: runnerdAvailable({ runners, count: runners.length }),
    state,
    reload,
  };
}
