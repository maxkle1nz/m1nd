/*
 * useGraphSnapshot — the reader's read of `/api/graph/snapshot`, the substrate the
 * outline + click-to-def are derived from (donor dossier: "the graph is the brain
 * of the reader"). Cached per brain root so opening Show Code on many files (or
 * re-opening the modal) fetches the snapshot at most once per brain — the honest
 * client-side alternative to a per-file backend view (dossier Risk 6; a thin
 * `GET /api/file/symbols?path=` owner view is the future optimization, recorded in
 * SLICE1-DIVERGENCES.md — this slice adds NO backend).
 *
 * SSR-safe like useFileView: the effect never runs in `renderToStaticMarkup`, and
 * an injected `override` lets a component test supply the snapshot with no network.
 */
import { useEffect, useState } from 'react';
import { api } from '../api/client';
import type { GraphSnapshot } from '../lib/snapshot';

const cache = new Map<string, Promise<GraphSnapshot>>();
const keyOf = (brain?: string | null): string => brain?.trim() || '__bound__';

/** Fetch the snapshot for a brain once and share the promise; a rejection evicts
 *  the key so a later mount can retry (never a poisoned cache). */
export function fetchGraphSnapshotCached(brain?: string | null): Promise<GraphSnapshot> {
  const k = keyOf(brain);
  let p = cache.get(k);
  if (!p) {
    p = api.graphSnapshot(brain);
    cache.set(k, p);
    p.catch(() => {
      if (cache.get(k) === p) cache.delete(k);
    });
  }
  return p;
}

/** Test-only: drop the cache so a fresh fetch is exercised. */
export function __clearGraphSnapshotCache(): void {
  cache.clear();
}

export type SnapshotStatus = 'idle' | 'loading' | 'ready' | 'error';

export interface GraphSnapshotState {
  snapshot: GraphSnapshot | null;
  status: SnapshotStatus;
}

/**
 * Read the (cached) graph snapshot for `brain`. `override` short-circuits the fetch
 * (SSR / component tests). `enabled=false` keeps it idle (the modal is closed).
 */
export function useGraphSnapshot(
  brain?: string | null,
  enabled: boolean = true,
  override?: GraphSnapshot | null,
): GraphSnapshotState {
  const [snapshot, setSnapshot] = useState<GraphSnapshot | null>(override ?? null);
  const [status, setStatus] = useState<SnapshotStatus>(override ? 'ready' : enabled ? 'loading' : 'idle');

  useEffect(() => {
    if (override) return; // injected — no fetch
    if (!enabled) {
      setStatus('idle');
      return;
    }
    let mounted = true;
    setStatus('loading');
    fetchGraphSnapshotCached(brain)
      .then((s) => {
        if (mounted) {
          setSnapshot(s);
          setStatus('ready');
        }
      })
      .catch(() => {
        if (mounted) setStatus('error');
      });
    return () => {
      mounted = false;
    };
  }, [brain, enabled, override]);

  return { snapshot: override ?? snapshot, status };
}
