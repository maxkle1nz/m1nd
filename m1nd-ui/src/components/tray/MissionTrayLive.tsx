/*
 * MissionTrayLive — the mission tray's data owner (HUMAN-VIEW-V2 F2.5 §3). Wires
 * `useMissions` (the §2b read for the viewed brain) and owns the presentation-local
 * state the tray keeps OUT of the box (§3c/§3e): the expand/collapse toggle and the
 * per-mission dismiss set (localStorage — a dismissed failure is un-pinned for THIS
 * human, the letter and ledger untouched). The provenance-open set is ephemeral.
 * Renders the pure MissionTray. Mounted in the App shell OUTSIDE the surface switch,
 * so it is fixed on every surface.
 *
 * §6-F2.5d — the human landing: this owner runs `landCandidate` (fresh snapshot →
 * `receipt_import` → the `landed` letter), all `?brain=`-scoped to the viewed brain,
 * then toasts the honest outcome and reloads the heads so the card flips to `landed`.
 * The click is guarded against re-entrancy per mission (the store is never double-hit).
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import type { ViewedBrain } from '../../lib/viewedBrain';
import { useMissions } from '../../hooks/useMissions';
import { api } from '../../api/client';
import { landCandidate, archiveHead, archiveBoundaryView, type MissionHead } from '../../lib/missions';
import { useToastStore } from '../../stores/toastStore';
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
  const { missions, status, error, reload } = useMissions(enabled, viewedBrain.root, expanded);
  const addToast = useToastStore((s) => s.addToast);
  // Re-entrancy guard: a mission being landed cannot be double-clicked into a second
  // `receipt_import` (the store would bump twice). A ref, not state — no re-render.
  const landingIds = useRef<Set<string>>(new Set());
  // The same guard for the archive gesture — a double-click cannot post two `archived`
  // letters (the second would `stale_head`, but guarding is cheaper than the round-trip).
  const archivingIds = useRef<Set<string>>(new Set());

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

  // §6-F2.5d — run the human landing for one head, scoped to the viewed brain: a fresh
  // snapshot → `receipt_import` → the `landed` letter. The honest outcome becomes a
  // toast (a success is `success`, a refusal `info`); a success/conflict reloads the
  // heads so the card flips to `landed` (or re-keys off the fresh store).
  const brainRoot = viewedBrain.root;
  const onImportReceipt = useCallback(
    async (head: MissionHead) => {
      if (landingIds.current.has(head.mission_id)) return;
      landingIds.current.add(head.mission_id);
      try {
        const result = await landCandidate(head, {
          snapshot: () => api.systemBlocksSnapshot(brainRoot),
          importReceipt: (input) => api.receiptImport(input, brainRoot),
          postMission: (letter) => api.missionPost(letter, brainRoot),
        });
        addToast(result.toast.text, undefined, result.landed ? 'success' : 'info');
        if (result.shouldReload) reload();
      } finally {
        landingIds.current.delete(head.mission_id);
      }
    },
    [brainRoot, reload, addToast],
  );

  // F2.5e — the archive confirm's FRESH two-boundary read (the landCandidate step-1
  // snapshot), scoped to the viewed brain. Derived, never stored — a moved boundary shows
  // live in the confirm ("proved at v1 — the block is at v3").
  const onArchivePreview = useCallback(
    async (head: MissionHead) => archiveBoundaryView(head, await api.systemBlocksSnapshot(brainRoot)),
    [brainRoot],
  );

  // F2.5e — post the terminal `archived` letter, scoped to the viewed brain, stamping the
  // human origin token (`archived_via:'human-ui'`) the backend requires. The honest
  // outcome toasts (success on archive, info on a `stale_head` race); a reload flips the
  // card to `archived` (or re-reads the moved head). Re-entrancy-guarded per mission.
  const onArchive = useCallback(
    async (head: MissionHead) => {
      if (archivingIds.current.has(head.mission_id)) return;
      archivingIds.current.add(head.mission_id);
      try {
        const result = await archiveHead(head, {
          postMission: (letter) => api.missionPost(letter, brainRoot, 'human-ui'),
        });
        addToast(result.toast.text, undefined, result.archived ? 'success' : 'info');
        if (result.shouldReload) reload();
      } finally {
        archivingIds.current.delete(head.mission_id);
      }
    },
    [brainRoot, reload, addToast],
  );

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
      onImportReceipt={onImportReceipt}
      onArchive={onArchive}
      onArchivePreview={onArchivePreview}
    />
  );
}
