/*
 * useBuildMap — the Build Map's read (HUMAN-VIEW-V2 F1). Fetches the ratified
 * SystemBlock store (`system_blocks_snapshot`) plus the graph snapshot (for the
 * members' persisted `xray:state:*` tags), then computes the PRD §5 rollup. All
 * read-only: the render never calls a mutating verb (the runtime overlay is NOT
 * touched here — F0-TECH §6). A graph-snapshot failure is best-effort: members
 * stay neutral ("not scanned yet"), never blanking the map. Mirrors useTreeData.
 *
 * §4A.9 (multi-brain owner): `brainRoot` routes BOTH reads to a hosted project
 * brain via the `?brain=` selector — on a multi-brain owner the BOUND brain is not
 * necessarily the one holding the skeleton, so a bound-only read leaves the map
 * blind. `null` = the bound brain (no selector — byte-compatible with F1). The
 * fetch heart is extracted as `loadBuildMap` so the routing is provable DOM-free
 * (the repo's liveRefreshCore pattern for hook tests).
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

export type MemberState = 'broken' | 'erosion' | 'ok';

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

/** The sinks `loadBuildMap` writes through — the hook's setters, injectable so the
 *  fetch heart is testable without a DOM (a test passes recorders). */
export interface BuildMapSinks {
  isMounted: () => boolean;
  setSnapshot: (snap: SystemBlocksSnapshot) => void;
  setMemberStates: (m: Map<string, MemberState>) => void;
  setStatus: (s: BuildMapStatus) => void;
  setError: (e: string) => void;
}

/**
 * The Build Map's fetch heart, extracted from the hook effect (behavior-identical:
 * same setter interleave, so the map still paints on the store BEFORE the graph
 * snapshot resolves). `brainRoot` rides both doors as the `?brain=` selector
 * (§4A.9) — `/api/tools/system_blocks_snapshot` and `/api/graph/snapshot` both
 * accept it on the owner; `null` leaves the URLs untouched (the bound brain).
 * A graph-snapshot failure keeps members neutral, never blanking the map.
 */
export async function loadBuildMap(brainRoot: string | null, sinks: BuildMapSinks): Promise<void> {
  try {
    const snap = await api.systemBlocksSnapshot(brainRoot);
    if (!sinks.isMounted()) return;
    sinks.setSnapshot(snap);
    if (!snap?.present || !snap.store) {
      sinks.setMemberStates(new Map());
      sinks.setStatus('empty');
      return;
    }
    sinks.setStatus('ready');
    // Best-effort xray tags for member states; a failure leaves members
    // neutral (the honest day-1 truth), never blanks the map.
    try {
      const graph = await api.graphSnapshot(brainRoot);
      if (sinks.isMounted()) sinks.setMemberStates(memberStatesFrom(graph));
    } catch {
      if (sinks.isMounted()) sinks.setMemberStates(new Map());
    }
  } catch (err) {
    if (!sinks.isMounted()) return;
    sinks.setError(err instanceof Error ? err.message : 'failed to load the build map');
    sinks.setStatus('error');
  }
}

export function useBuildMap(
  enabled: boolean = true,
  brainRoot: string | null = null,
  refreshKey = 0,
): BuildMapData {
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
    void loadBuildMap(brainRoot, {
      isMounted: () => mounted,
      setSnapshot,
      setMemberStates,
      setStatus,
      setError,
    });
    return () => {
      mounted = false;
    };
  }, [enabled, brainRoot, tick, refreshKey]);

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
