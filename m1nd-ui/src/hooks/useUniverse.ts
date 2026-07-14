/*
 * useUniverse — the L0 panorama's read (HUMAN-VIEW-V2 F30). Polls
 * `GET /api/universe` (~5s nerve, like the Hall's brains/self reads) and exposes
 * a settled `status` the landing decision gates on. Pure-degradation: a pre-F30
 * owner (404) or any read error resolves to a READY, EMPTY panorama — never a
 * loading hang — so the entry rule falls through to the prior doctrine untouched
 * (the byte-compatible posture the Hall's feature-detections already use).
 */
import { useCallback, useEffect, useState } from 'react';
import { api } from '../api/client';
import type { UniverseResponse } from '../lib/universe';

export type UniverseStatus = 'loading' | 'ready' | 'error';

const EMPTY: UniverseResponse = {
  schema: 'm1nd-universe-v0',
  worlds: [],
  owner: { alerts_pending: 0 },
  totals: { worlds: 0, awake: 0, pending: 0 },
};

export interface UniverseData {
  status: UniverseStatus;
  universe: UniverseResponse;
  reload: () => void;
}

export function useUniverse(enabled: boolean): UniverseData {
  const [status, setStatus] = useState<UniverseStatus>('loading');
  const [universe, setUniverse] = useState<UniverseResponse>(EMPTY);
  const [tick, setTick] = useState(0);
  const reload = useCallback(() => setTick((t) => t + 1), []);

  useEffect(() => {
    if (!enabled) return;
    let mounted = true;
    const poll = () =>
      api
        .universe()
        .then((u) => {
          if (!mounted) return;
          // Defensive against a partial/legacy body: never a loading hang.
          setUniverse({
            schema: u.schema ?? EMPTY.schema,
            worlds: Array.isArray(u.worlds) ? u.worlds : [],
            owner: u.owner ?? EMPTY.owner,
            totals: u.totals ?? EMPTY.totals,
          });
          setStatus('ready');
        })
        .catch(() => {
          // A pre-F30 owner (404) or a transient error → honest-empty + settled,
          // so the landing never waits on a route that will never answer.
          if (!mounted) return;
          setUniverse(EMPTY);
          setStatus('ready');
        });
    poll();
    const id = setInterval(poll, 5000);
    return () => {
      mounted = false;
      clearInterval(id);
    };
  }, [enabled, tick]);

  return { status, universe, reload };
}
