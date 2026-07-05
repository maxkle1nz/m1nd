/*
 * useTreeData — loads the Living Tree from the live served endpoints (PRD §3.6).
 * Snapshot → tree skeleton + post-it index. trust → per-node band (nodes with no
 * learn history honestly default to insufficient_evidence, INV-02). tremor →
 * churning rows breathe. All read-only.
 *
 * §4A.9: every fetch carries the viewed brain's `?brain=` selector (absent = the
 * bound graph). The snapshot's `served_brain` echo is ASSERTED against the brain
 * we opened — a mismatch is DROPPED, never rendered under the wrong chip (INV-15).
 * A dormant hosted brain warm-boots on the first fetch: the status says so in
 * words ("waking", INV-05), never a fake bar.
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import { api } from '../api/client';
import type { TrustResult, TremorResult } from '../api/toolTypes';
import type { GraphSnapshot } from '../lib/snapshot';
import { buildTree, type TreeRow } from '../lib/tree';
import { tierToBand, type TrustBand } from '../lib/softProof';
import { BOUND_VIEW, brainSelectorFor, servedBrainMatches, type ViewedBrain } from '../lib/viewedBrain';

// A hosted brain's first fetch may warm-boot a dormant store; after this long the
// tree says "waking this brain…" in words (INV-05) instead of a silent wait.
const WAKING_HINT_MS = 400;

export type TreeStatus = 'loading' | 'waking' | 'ready' | 'needs_ingest' | 'error';

export interface TreeData {
  status: TreeStatus;
  root: TreeRow | null;
  /** external_id → trust band. Missing entry means insufficient_evidence. */
  bands: Map<string, TrustBand>;
  /** source_path of nodes currently churning (tremor). */
  breathingPaths: Set<string>;
  /** The brain that actually answered (the snapshot's `served_brain` echo). The
   *  Brain Chip renders THIS (truth), not the request. Null on a pre-2H owner. */
  servedBrain: GraphSnapshot['served_brain'] | null;
  error: string | null;
  reload: () => void;
}

export function bandFor(bands: Map<string, TrustBand>, externalId: string | undefined): TrustBand {
  if (!externalId) return 'insufficient_evidence';
  return bands.get(externalId) ?? 'insufficient_evidence';
}

export function useTreeData(view: ViewedBrain = BOUND_VIEW, refreshKey = 0): TreeData {
  const [status, setStatus] = useState<TreeStatus>('loading');
  const [snapshot, setSnapshot] = useState<GraphSnapshot | null>(null);
  const [bands, setBands] = useState<Map<string, TrustBand>>(new Map());
  const [breathingPaths, setBreathingPaths] = useState<Set<string>>(new Set());
  const [servedBrain, setServedBrain] = useState<GraphSnapshot['served_brain'] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tick, setTick] = useState(0);

  const reload = useCallback(() => setTick((t) => t + 1), []);
  const brain = brainSelectorFor(view);

  useEffect(() => {
    let mounted = true;
    setStatus('loading');
    setError(null);
    // Only a hosted brain can be dormant; the "waking" hint is for it alone.
    const wakingTimer =
      view.root != null
        ? setTimeout(() => {
            if (mounted) setStatus((s) => (s === 'loading' ? 'waking' : s));
          }, WAKING_HINT_MS)
        : null;

    (async () => {
      try {
        const snap = await api.graphSnapshot(brain);
        if (!mounted) return;

        // INV-15: the tree renders a brain's nodes ONLY under that brain's chip.
        // A response whose echo names a DIFFERENT brain (an owner that ignored the
        // selector, or a stale response across an Open switch) is dropped whole.
        if (!servedBrainMatches(view, snap?.served_brain)) {
          setError(
            'this owner served a different brain than the one you opened — dropped it rather than show the wrong map.',
          );
          setStatus('error');
          setSnapshot(null);
          return;
        }

        setServedBrain(snap?.served_brain ?? null);

        if (!snap || snap.nodes.length === 0) {
          setStatus('needs_ingest');
          setSnapshot(null);
          return;
        }
        setSnapshot(snap);
        setStatus('ready');

        // Trust + tremor are best-effort decorations; a failure must not blank
        // the tree (bands simply stay insufficient_evidence). Both ride the same
        // selector so they answer from the VIEWED brain (INV-16).
        try {
          const trust = await api.tool<TrustResult>(
            'trust',
            { scope: 'all', sort_by: 'trust_asc', limit: 200 },
            brain,
          );
          if (!mounted) return;
          const m = new Map<string, TrustBand>();
          for (const t of trust.trust_scores ?? []) m.set(t.node_id, tierToBand(t.tier));
          setBands(m);
        } catch {
          if (mounted) setBands(new Map());
        }

        try {
          const tremor = await api.tool<TremorResult>('tremor', {}, brain);
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
      if (wakingTimer) clearTimeout(wakingTimer);
    };
    // `brain` fully captures the viewed root for the fetch; view identity changes
    // when the root does. eslint sees view.root as the honest dep.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tick, refreshKey, brain]);

  const root = useMemo(() => (snapshot ? buildTree(snapshot) : null), [snapshot]);

  return { status, root, bands, breathingPaths, servedBrain, error, reload };
}
