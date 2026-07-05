/*
 * useCardV2Data — fetch the card-v2 GOLD/DEPTH fields for the OPEN brain
 * (HUMAN-LAYER-PRD §4A.3.1). All REST-reachable TODAY for the bound brain via
 * POST /api/tools/* (the same dispatch_tool as stdio).
 *
 * Cost discipline (the §5.4 measure-first posture): G1 freshness (am_i_stale,
 * one hash per file) and G2 calibration (predict) are computed ON DEMAND when the
 * open card is shown — NEVER a background poll of every card. The snapshot (G3/D1)
 * is fetched once; sessions/queries (G4) come from the self envelope the Hall
 * already polls. Everything is absent-honest until it resolves.
 */
import { useEffect, useState } from 'react';
import { api } from '../api/client';
import type { AmIStaleOutput } from '../api/toolTypes';
import type { GraphSnapshot } from '../lib/snapshot';
import type { InstanceSelfResponse } from '../types';
import {
  freshnessG1,
  calibrationG2,
  compoundingG3,
  alivenessG4,
  lastClaimD1,
  honestGapsD2,
  type FreshnessG1,
  type CalibrationG2,
  type CompoundingG3,
  type AlivenessG4,
  type LastClaimD1,
  type HonestGapsD2,
  type CalibrationBlock,
} from '../lib/cardV2';
import { collectLeafRows } from '../lib/treeLenses';
import { buildTree } from '../lib/tree';

export interface CardV2Data {
  g1: FreshnessG1 | null;
  g2: CalibrationG2 | null;
  g3: CompoundingG3 | null;
  g4: AlivenessG4 | null;
  d1: LastClaimD1 | null;
  d2: HonestGapsD2 | null;
  /** trigger G1 recompute (after a Re-read). */
  refreshFreshness: () => void;
}

const EMPTY: Omit<CardV2Data, 'refreshFreshness'> = {
  g1: null,
  g2: null,
  g3: null,
  g4: null,
  d1: null,
  d2: null,
};

/**
 * Compute the open brain's card-v2 fields. `enabled` gates the on-demand cost —
 * pass true only for the OPEN/bound brain's card or its receipt. A hosted brain
 * passes false and every field stays absent-honest.
 */
export function useCardV2Data(enabled: boolean, self: InstanceSelfResponse | null): CardV2Data {
  const [data, setData] = useState(EMPTY);
  const [tick, setTick] = useState(0);

  // G4 is free (the self envelope the Hall already polls). PARTITIONED (R14 /
  // §9.5.1): the self envelope's `active_agent_sessions`/`queries_processed` ARE
  // the bound brain's OWN counters — routed project-brain calls dispatch against
  // that brain's SessionState, never the owner's, so these count only the bound
  // brain's traffic (the owner-WIDE total lives on the receipt, labeled so). The
  // "across all brains" qualifier is gone; the count is truly this brain's.
  useEffect(() => {
    setData((d) => ({ ...d, g4: alivenessG4(self ? { attached_sessions: self.active_agent_sessions, query_count: self.queries_processed } : null) }));
  }, [self]);

  // Snapshot-derived G3 + D1 (one fetch when the card opens).
  useEffect(() => {
    if (!enabled) return;
    let mounted = true;
    api
      .graphSnapshot()
      .then((snap: GraphSnapshot) => {
        if (!mounted) return;
        setData((d) => ({ ...d, g3: compoundingG3(snap), d1: lastClaimD1(snap) }));
      })
      .catch(() => {});
    return () => {
      mounted = false;
    };
  }, [enabled]);

  // G2 calibration (predict — one call, on demand).
  useEffect(() => {
    if (!enabled) return;
    let mounted = true;
    (async () => {
      try {
        // predict needs a changed_node; use a PageRank-ish anchor from the snapshot.
        const snap = await api.graphSnapshot();
        const anchor = snap.nodes.find((n) => n.external_id.includes('::struct::') || n.external_id.includes('::fn::'))?.external_id;
        if (!anchor) return;
        const res = await api.tool<{ calibration?: CalibrationBlock }>('predict', { changed_node: anchor });
        if (mounted && res.calibration) setData((d) => ({ ...d, g2: calibrationG2(res.calibration!) }));
      } catch {
        /* absent-honest */
      }
    })();
    return () => {
      mounted = false;
    };
  }, [enabled]);

  // G1 freshness (am_i_stale over the visible file set — on demand, recomputes on tick).
  useEffect(() => {
    if (!enabled) return;
    let mounted = true;
    (async () => {
      try {
        const snap = await api.graphSnapshot();
        const paths = collectLeafRows(buildTree(snap))
          .filter((r) => r.kind === 'file')
          .map((r) => r.path);
        const res = await api.tool<AmIStaleOutput>('am_i_stale', { paths });
        if (mounted) setData((d) => ({ ...d, g1: freshnessG1(res) }));
      } catch {
        /* absent-honest */
      }
    })();
    return () => {
      mounted = false;
    };
  }, [enabled, tick]);

  // D2 honest gaps (orient coverage + ghost edges + north gaps).
  useEffect(() => {
    if (!enabled) return;
    let mounted = true;
    (async () => {
      try {
        const north = await api.tool<{ context?: { coverage?: { visited?: number; total?: number } }; honest_gaps?: string[] }>('north', {
          task: 'card receipt coverage',
        });
        let ghost: number | null = null;
        try {
          const g = await api.tool<{ ghost_edges?: unknown[] } & Record<string, unknown>>('ghost_edges', {});
          const arr = (g.ghost_edges ?? (g as { edges?: unknown[] }).edges) as unknown[] | undefined;
          ghost = Array.isArray(arr) ? arr.length : null;
        } catch {
          /* ghost edges absent-honest */
        }
        if (mounted) {
          setData((d) => ({
            ...d,
            d2: honestGapsD2({ coverage: north.context?.coverage, ghostEdges: ghost, gaps: north.honest_gaps }),
          }));
        }
      } catch {
        /* absent-honest */
      }
    })();
    return () => {
      mounted = false;
    };
  }, [enabled]);

  return { ...data, refreshFreshness: () => setTick((t) => t + 1) };
}
