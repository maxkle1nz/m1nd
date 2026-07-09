/*
 * MissionTrayLive — the mission tray's data owner (HUMAN-VIEW-V2 F2.5 §3). Wires
 * `useMissions` (the §2b read for the viewed brain) and owns the presentation-local
 * state the tray keeps OUT of the box (§3c/§3e): the expand/collapse toggle and the
 * per-mission dismiss set (localStorage — a dismissed failure is un-pinned for THIS
 * human, the letter and ledger untouched). The provenance-open set is ephemeral.
 * Renders the pure MissionTray. Mounted in the App shell OUTSIDE the surface switch,
 * so it is fixed on every surface.
 */
import { useCallback, useEffect, useState } from 'react';
import type { ViewedBrain } from '../../lib/viewedBrain';
import { useMissions } from '../../hooks/useMissions';
import MissionTray from './MissionTray';

const EXPANDED_KEY = 'm1nd:mission-tray:expanded';
const DISMISSED_KEY = 'm1nd:mission-tray:dismissed';

function readExpanded(): boolean {
  try {
    return globalThis.localStorage?.getItem(EXPANDED_KEY) === '1';
  } catch {
    return false;
  }
}

function readDismissed(): Set<string> {
  try {
    const raw = globalThis.localStorage?.getItem(DISMISSED_KEY);
    const arr = raw ? (JSON.parse(raw) as unknown) : [];
    return new Set(Array.isArray(arr) ? arr.filter((x): x is string => typeof x === 'string') : []);
  } catch {
    return new Set();
  }
}

export interface MissionTrayLiveProps {
  viewedBrain: ViewedBrain;
  enabled: boolean;
  onOpenBlock: (blockId: string) => void;
}

export default function MissionTrayLive({ viewedBrain, enabled, onOpenBlock }: MissionTrayLiveProps) {
  const [expanded, setExpanded] = useState<boolean>(readExpanded);
  const [dismissed, setDismissed] = useState<Set<string>>(readDismissed);
  const [provenance, setProvenance] = useState<Set<string>>(new Set());
  const { missions, status, error } = useMissions(enabled, viewedBrain.root, expanded);

  // Persist the expand state so the tray remembers how the human left it.
  useEffect(() => {
    try {
      globalThis.localStorage?.setItem(EXPANDED_KEY, expanded ? '1' : '0');
    } catch {
      /* private mode / no storage — the toggle still works in-session. */
    }
  }, [expanded]);

  const onToggleExpanded = useCallback(() => setExpanded((e) => !e), []);

  const onDismiss = useCallback((missionId: string) => {
    setDismissed((prev) => {
      const next = new Set(prev);
      next.add(missionId);
      try {
        globalThis.localStorage?.setItem(DISMISSED_KEY, JSON.stringify([...next]));
      } catch {
        /* non-fatal — the dismiss holds for this session regardless. */
      }
      return next;
    });
  }, []);

  const onToggleProvenance = useCallback((missionId: string) => {
    setProvenance((prev) => {
      const next = new Set(prev);
      if (next.has(missionId)) next.delete(missionId);
      else next.add(missionId);
      return next;
    });
  }, []);

  return (
    <MissionTray
      missions={missions}
      status={status}
      error={error}
      expanded={expanded}
      onToggleExpanded={onToggleExpanded}
      dismissedIds={dismissed}
      onDismiss={onDismiss}
      provenanceIds={provenance}
      onToggleProvenance={onToggleProvenance}
      onOpenBlock={onOpenBlock}
    />
  );
}
