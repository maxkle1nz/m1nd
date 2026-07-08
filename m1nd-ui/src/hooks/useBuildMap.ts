/*
 * useBuildMap — the Build Map's read (HUMAN-VIEW-V2 F1). Fetches the ratified
 * SystemBlock store (`system_blocks_snapshot`) plus the graph snapshot (for the
 * members' persisted `xray:state:*` tags), then computes the PRD §5 rollup. All
 * read-only: the render never calls a mutating verb (the runtime overlay is NOT
 * touched here — F0-TECH §6). A graph-snapshot failure is best-effort: members
 * stay neutral ("not scanned yet"), never blanking the map. Mirrors useTreeData.
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import { api } from '../api/client';
import type { GraphSnapshot } from '../lib/snapshot';
import {
  rollupStore,
  type MapRollup,
  type SystemBlockStore,
  type SystemBlocksSnapshot,
} from '../lib/buildMap';

export type BuildMapStatus = 'loading' | 'ready' | 'empty' | 'error';

export interface BuildMapData {
  status: BuildMapStatus;
  present: boolean;
  snapshot: SystemBlocksSnapshot | null;
  store: SystemBlockStore | null;
  rollup: MapRollup | null;
  honest: string | null;
  error: string | null;
  reload: () => void;
}

type MemberState = 'broken' | 'erosion' | 'ok';

/**
 * Read repo-relative path → xray state from the graph snapshot's persisted
 * `xray:state:*` tags (PRD §5: the render reads tags the snapshot already carries;
 * it never calls the mutating overlay). Broken/erosion dominate a prior 'ok'.
 * Absent tags → an empty map (the honest day-1 "not scanned"). Members declared
 * as globs are resolved by a later phase (F0c/F2); exact-path members bind now.
 */
export function memberStatesFrom(snap: GraphSnapshot | null): Map<string, MemberState> {
  const m = new Map<string, MemberState>();
  if (!snap) return m;
  for (const node of snap.nodes) {
    const path = node.provenance?.source_path;
    if (!path) continue;
    const tag = node.tags.find((t) => t.startsWith('xray:state:'));
    if (!tag) continue;
    const suffix = tag.slice('xray:state:'.length);
    const st: MemberState = suffix.includes('broken')
      ? 'broken'
      : suffix.includes('erosion')
        ? 'erosion'
        : 'ok';
    const prev = m.get(path);
    if (prev === 'broken') continue;
    if (prev === 'erosion' && st === 'ok') continue;
    m.set(path, st);
  }
  return m;
}

export function useBuildMap(enabled: boolean = true, refreshKey = 0): BuildMapData {
  const [status, setStatus] = useState<BuildMapStatus>('loading');
  const [snapshot, setSnapshot] = useState<SystemBlocksSnapshot | null>(null);
  const [memberStates, setMemberStates] = useState<Map<string, MemberState>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  const reload = useCallback(() => setTick((t) => t + 1), []);

  useEffect(() => {
    if (!enabled) return;
    let mounted = true;
    setStatus('loading');
    setError(null);
    (async () => {
      try {
        const snap = await api.systemBlocksSnapshot();
        if (!mounted) return;
        setSnapshot(snap);
        if (!snap?.present || !snap.store) {
          setMemberStates(new Map());
          setStatus('empty');
          return;
        }
        setStatus('ready');
        // Best-effort xray tags for member states; a failure leaves members
        // neutral (the honest day-1 truth), never blanks the map.
        try {
          const graph = await api.graphSnapshot();
          if (mounted) setMemberStates(memberStatesFrom(graph));
        } catch {
          if (mounted) setMemberStates(new Map());
        }
      } catch (err) {
        if (!mounted) return;
        setError(err instanceof Error ? err.message : 'failed to load the build map');
        setStatus('error');
      }
    })();
    return () => {
      mounted = false;
    };
  }, [enabled, tick, refreshKey]);

  const store = snapshot?.present ? snapshot.store ?? null : null;
  const rollup = useMemo(() => (store ? rollupStore(store, memberStates) : null), [store, memberStates]);

  return {
    status,
    present: snapshot?.present ?? false,
    snapshot,
    store,
    rollup,
    honest: snapshot?.honest ?? null,
    error,
    reload,
  };
}
