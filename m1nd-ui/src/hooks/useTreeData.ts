/*
 * useTreeData — loads the Living Tree from the live served endpoints (PRD §3.6).
 * Snapshot → tree skeleton + post-it index. trust → per-node band (nodes with no
 * learn history honestly default to insufficient_evidence, INV-02). tremor →
 * churning rows breathe. All read-only.
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import { api } from '../api/client';
import type { TrustResult, TremorResult } from '../api/toolTypes';
import type { GraphSnapshot } from '../lib/snapshot';
import { buildTree, type TreeRow } from '../lib/tree';
import { tierToBand, type TrustBand } from '../lib/softProof';

export type TreeStatus = 'loading' | 'ready' | 'needs_ingest' | 'error';

export interface TreeData {
  status: TreeStatus;
  root: TreeRow | null;
  /** external_id → trust band. Missing entry means insufficient_evidence. */
  bands: Map<string, TrustBand>;
  /** source_path of nodes currently churning (tremor). */
  breathingPaths: Set<string>;
  error: string | null;
  reload: () => void;
}

export function bandFor(bands: Map<string, TrustBand>, externalId: string | undefined): TrustBand {
  if (!externalId) return 'insufficient_evidence';
  return bands.get(externalId) ?? 'insufficient_evidence';
}

export function useTreeData(refreshKey = 0): TreeData {
  const [status, setStatus] = useState<TreeStatus>('loading');
  const [snapshot, setSnapshot] = useState<GraphSnapshot | null>(null);
  const [bands, setBands] = useState<Map<string, TrustBand>>(new Map());
  const [breathingPaths, setBreathingPaths] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [tick, setTick] = useState(0);

  const reload = useCallback(() => setTick((t) => t + 1), []);

  useEffect(() => {
    let mounted = true;
    setStatus('loading');
    setError(null);

    (async () => {
      try {
        const snap = await api.graphSnapshot();
        if (!mounted) return;
        if (!snap || snap.nodes.length === 0) {
          setStatus('needs_ingest');
          setSnapshot(null);
          return;
        }
        setSnapshot(snap);
        setStatus('ready');

        // Trust + tremor are best-effort decorations; a failure must not blank
        // the tree (bands simply stay insufficient_evidence).
        try {
          const trust = await api.tool<TrustResult>('trust', {
            scope: 'all',
            sort_by: 'trust_asc',
            limit: 200,
          });
          if (!mounted) return;
          const m = new Map<string, TrustBand>();
          for (const t of trust.trust_scores ?? []) m.set(t.node_id, tierToBand(t.tier));
          setBands(m);
        } catch {
          if (mounted) setBands(new Map());
        }

        try {
          const tremor = await api.tool<TremorResult>('tremor', {});
          if (!mounted) return;
          const paths = new Set<string>();
          for (const e of tremor.tremors ?? []) {
            const id = e.node_id;
            if (!id) continue;
            const node = snap.nodes.find((n) => n.external_id === id);
            const sp = node?.provenance.source_path;
            if (sp) paths.add(sp);
          }
          setBreathingPaths(paths);
        } catch {
          if (mounted) setBreathingPaths(new Set());
        }
      } catch (err) {
        if (!mounted) return;
        setError(err instanceof Error ? err.message : 'failed to load graph');
        setStatus('error');
      }
    })();

    return () => {
      mounted = false;
    };
  }, [tick, refreshKey]);

  const root = useMemo(() => (snapshot ? buildTree(snapshot) : null), [snapshot]);

  return { status, root, bands, breathingPaths, error, reload };
}
