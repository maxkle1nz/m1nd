/*
 * HallView — the Hall, rung −1 (HUMAN-LAYER-PRD §4A.3).
 *
 * The front door of the OWNER: every brain it holds, one glance. Promotes and
 * re-skins the retired InstancesPanel (§4A.3, research R1) to SOFT PROOF — the
 * cyberpunk tokens die here. Three brain classes from the owner's own registry
 * (self + siblings + hosted project brains), freshest-first as the registry
 * already sorts them (never re-sorted — INV-10 / R4). Live the way the tree is:
 * graph_changed SSE → debounced refetch → quiet in-place update (useLiveRefresh
 * reused, R7). The card face carries ≤5 facts; the receipt lives in the drawer.
 *
 * Keyboard: roving focus over the grid (↑/↓), Enter selects, ESC ascends (the
 * Hall is rung −1 — ESC here leaves the Hall, handled by the parent ladder).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api, ApiError } from '../../api/client';
import type { InstanceRegistryEntry, InstanceListResponse, InstanceSelfResponse } from '../../types';
import { useLiveRefresh } from '../../hooks/useLiveRefresh';
import { usePresences } from '../../hooks/usePresences';
import { useToastStore } from '../../stores/toastStore';
import {
  entryBaseUrl,
  brainDisplayName,
  brainProjectPath,
  isProjectBrain,
  groupBrainCards,
} from '../../lib/hallSemantics';
import { canonRootForCompare } from '../../lib/viewedBrain';
import { useCardV2Data } from '../../hooks/useCardV2Data';
import { Icon } from '../../lib/icons/registry';
import { unackedAlerts, type DaemonAlert } from '../../lib/alerts';
import BrainCard from './BrainCard';
import BrainReceiptDrawer from './BrainReceiptDrawer';
import MailboxView from './MailboxView';
import OwnerAlertsPanel from './OwnerAlertsPanel';
import PresenceStrip from './PresenceStrip';

interface HallViewProps {
  /** ESC at the Hall (rung −1) ascends out of it — the parent owns where to. */
  onExit?: () => void;
  /** Open the bound brain's tree (self card / bound Open). */
  onOpenBound: () => void;
  /**
   * Open a brain IN THE TREE (§4A.9): the bound/self brain, OR a hosted project
   * brain (via the `?brain=` selector). The parent sets the viewed brain and
   * switches to the tree. A live SIBLING (own port) still opens in a new tab here.
   */
  onOpenBrain: (entry: InstanceRegistryEntry, isSelf: boolean) => void;
  /** Start the Threshold bootstrap ("+ Read a new repo"). */
  onBootstrap: () => void;
  /** §4A.9.5 capability stamp — enables Open on hosted project cards when true. */
  restSelector: boolean;
  /** The root the tree is currently viewing (§4A.8) — the card wearing the viewing
   *  chip. `null` = the bound brain. */
  viewedRoot: string | null;
  /** Honest doors: the Universe Landing's OWNER item routes here (`onOpenOwner`) and
   *  asks the Hall to open on its owner-alerts panel. Any other Hall entry leaves it
   *  closed. Read once at mount (the Hall remounts on each entry). */
  autoOpenAlerts?: boolean;
}

function errorDetail(error: unknown, fallback: string): string {
  if (error instanceof ApiError) return error.detail;
  if (error instanceof Error) return error.message;
  return fallback;
}

export default function HallView({
  onExit,
  onOpenBrain,
  onBootstrap,
  restSelector,
  viewedRoot,
  autoOpenAlerts = false,
}: HallViewProps) {
  const [self, setSelf] = useState<InstanceSelfResponse | null>(null);
  const [instances, setInstances] = useState<InstanceRegistryEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [savingId, setSavingId] = useState<string | null>(null);
  // The brain whose Mailbox (§4A.11) is open beside the grid, or null. The medulla
  // box is addressed by the literal `medulla` root; a project brain by its root.
  const [mailboxFor, setMailboxFor] = useState<{ root: string; name: string | null } | null>(null);
  // The owner's daemon alerts (honest doors) — the BOUND session's stock, the same one
  // the Universe counts. Fetched fail-open (a pre-alerts / read-only owner shows none,
  // never a wall). The panel opens beside the grid; the Landing's owner item lands here.
  const [alerts, setAlerts] = useState<DaemonAlert[]>([]);
  const [alertsActive, setAlertsActive] = useState(true);
  const [alertsOpen, setAlertsOpen] = useState(false);
  const gridRef = useRef<HTMLDivElement>(null);
  const addToast = useToastStore((s) => s.addToast);
  // The presence strip (P1) — the OWNER-WIDE roster (no brain = the control-room
  // scope). Its own ~5s nerve + the shared live-refresh; fails open to empty.
  const presence = usePresences(true);

  const refresh = useCallback(async () => {
    try {
      const [selfResp, listResp] = await Promise.all([api.instanceSelf(), api.instances()]);
      setSelf(selfResp);
      // Registry order IS recency — never re-sort (R4/INV-10).
      setInstances((listResp as InstanceListResponse).instances);
      setError((listResp as InstanceListResponse).error ?? null);
    } catch (err) {
      setError(errorDetail(err, 'Failed to load the Hall'));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // The owner-alerts read — BOUND session only (no `?brain=`), fail-open so a
  // read-only / pre-alerts owner shows an empty panel, never an error wall.
  const refreshAlerts = useCallback(async () => {
    try {
      const resp = await api.alertsList();
      setAlerts(resp.alerts);
      setAlertsActive(resp.active);
    } catch {
      setAlerts([]);
    }
  }, []);
  useEffect(() => {
    void refreshAlerts();
  }, [refreshAlerts]);

  // Live refresh (R7) — same calm nerve as the tree (brains + alerts together).
  const onLive = useCallback(() => {
    void refresh();
    void refreshAlerts();
  }, [refresh, refreshAlerts]);
  useLiveRefresh({ onRefresh: onLive, enabled: true });

  // Ack is presentation of a real WRITE: `alerts_ack` flips the owner's alert and the
  // count falls in lockstep. Bound-session scope; a refusal (read-only) surfaces honestly.
  const onAckAlerts = useCallback(
    async (ids: string[]) => {
      if (ids.length === 0) return;
      try {
        await api.alertsAck(ids);
        await refreshAlerts();
      } catch (err) {
        setError(errorDetail(err, 'Acknowledge failed'));
      }
    },
    [refreshAlerts],
  );

  // The Landing's owner item lands ON the alerts panel: open it once at mount when the
  // Hall was entered via `onOpenOwner` (any other entry leaves it closed).
  useEffect(() => {
    if (autoOpenAlerts) setAlertsOpen(true);
  }, [autoOpenAlerts]);

  const pendingAlerts = unackedAlerts(alerts).length;

  const selfId = self?.instance.instance_id ?? null;
  // Defense in depth (§4A.3): collapse duplicate registry entries to ONE card per
  // workspace. The backend stable-id fix removes duplicates at the source; this
  // guarantees the Hall never renders the "N identical cards" symptom regardless
  // of what the owner reports (freshest per workspace wins; order preserved).
  const cards = useMemo(() => groupBrainCards(instances), [instances]);
  const selected = useMemo(
    () => cards.find((c) => c.entry.instance_id === selectedId)?.entry ?? null,
    [cards, selectedId],
  );

  // Card-v2 GOLD/DEPTH for the OPEN/bound brain (§4A.3.1). On-demand only — the
  // Hall opens on the bound brain, so `enabled` is simply "we have a self".
  const v2 = useCardV2Data(self != null, self);

  const reread = useCallback(async () => {
    const root = self?.project_root;
    if (!root) return;
    try {
      await api.tool('ingest', { path: root });
      addToast('re-reading this repo…', brainDisplayName({ display_name: self?.display_name, workspace_root: root }), 'info');
      v2.refreshFreshness();
    } catch (err) {
      setError(errorDetail(err, 'Re-read failed'));
    }
  }, [self, addToast, v2]);

  const calibrate = useCallback(async () => {
    try {
      await api.tool('calibrate_predict', {});
      addToast('calibrating on this repo…', 'measuring precision once', 'info');
    } catch (err) {
      setError(errorDetail(err, 'Calibrate failed'));
    }
  }, [addToast]);

  const openBrain = useCallback(
    (entry: InstanceRegistryEntry) => {
      const isSelf = entry.instance_id === selfId;
      // The bound/self brain and hosted PROJECT brains open in the tree (§4A.9):
      // hand the brain up so the parent sets the viewed brain + shows the tree.
      if (isSelf || isProjectBrain(entry)) {
        onOpenBrain(entry, isSelf);
        return;
      }
      // A live SIBLING owner serves its own UI — open it in a new tab (§4A.4).
      const url = entryBaseUrl(entry);
      if (url) window.open(url, '_blank', 'noopener,noreferrer');
    },
    [selfId, onOpenBrain],
  );

  // Open a brain's caixinha (§4A.11): the D3 "N open" entry. The box is that
  // repo's repo-side file, addressed by its project root (the medulla box is a
  // separate, labeled entry point — not a card's D3).
  const openMailbox = useCallback((entry: InstanceRegistryEntry) => {
    const root = brainProjectPath(entry);
    if (!root) return;
    setMailboxFor({ root, name: brainDisplayName(entry) });
  }, []);

  const saveBrain = useCallback(
    async (entry: InstanceRegistryEntry) => {
      setSavingId(entry.instance_id);
      try {
        if (entry.instance_id === selfId) await api.saveSelfInstanceState();
        else await api.saveInstanceState(entry.instance_id);
        addToast('state saved', brainDisplayName(entry), 'info');
      } catch (err) {
        setError(errorDetail(err, 'Save failed'));
      } finally {
        setSavingId(null);
      }
    },
    [selfId, addToast],
  );

  const onDeleted = useCallback(
    (entry: InstanceRegistryEntry) => {
      setSelectedId(null);
      setInstances((prev) => prev.filter((e) => e.instance_id !== entry.instance_id));
      void refresh();
    },
    [refresh],
  );

  // Roving focus over the grid (§4A.5): ↑/↓ move selection; ESC ascends.
  const onGridKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (selected) setSelectedId(null);
        else onExit?.();
        return;
      }
      if (cards.length === 0) return;
      const idx = cards.findIndex((x) => x.entry.instance_id === selectedId);
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        const next = cards[Math.min(idx + 1, cards.length - 1)] ?? cards[0];
        setSelectedId(next.entry.instance_id);
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        const prev = cards[Math.max(idx - 1, 0)] ?? cards[0];
        setSelectedId(prev.entry.instance_id);
      }
    },
    [cards, selectedId, selected, onExit],
  );

  return (
    <div className="flex-1 flex overflow-hidden bg-porcelain" data-surface="hall">
      <div className="flex-1 flex flex-col min-w-0">
        {/* Header */}
        <div className="px-6 py-4 border-b border-ink/10 flex items-center justify-between gap-4">
          <div>
            <h1 className="text-lg text-ink font-semibold">Your brains</h1>
            <p className="text-xs text-ink-soft mt-0.5">
              Every map m1nd holds for you. {cards.length} {cards.length === 1 ? 'brain' : 'brains'}.
            </p>
          </div>
          <div className="flex items-center gap-2">
            {/* Owner alerts (honest doors) — the owner-daemon's findings, where the
                Universe Landing's owner item lands. The badge counts UNACKED alerts
                (the same stock the Universe counts); absent when the bell is quiet. */}
            <button
              type="button"
              data-role="owner-alerts-entry"
              onClick={() => setAlertsOpen(true)}
              className="inline-flex items-center gap-1 px-3 py-1.5 text-xs bg-bone text-ink-soft border border-ink/15 rounded hover:text-ink hover:shadow-contact transition-shadow"
              title="Owner daemon alerts — acknowledge from here"
            >
              <Icon name="inbox" size={14} decorative className="text-ink-soft/80" />
              Alerts
              {pendingAlerts > 0 && (
                <span
                  data-role="owner-alerts-badge"
                  className="ml-0.5 text-[10px] font-mono px-1.5 py-px rounded-full border border-verdict-reverify/50 bg-verdict-reverify-tint/40 text-ink tabular-nums"
                >
                  {pendingAlerts}
                </span>
              )}
            </button>
            {/* The medulla box (§4A.11) — its own labeled entry, holding ONLY the
                projectless letters (transversal tools, owner runtime). Opened by
                the literal `medulla` selector, never mixed into a project box. */}
            <button
              type="button"
              data-role="medulla-entry"
              onClick={() => setMailboxFor({ root: 'medulla', name: 'medulla' })}
              className="inline-flex items-center gap-1 px-3 py-1.5 text-xs bg-bone text-ink-soft border border-ink/15 rounded hover:text-ink hover:shadow-contact transition-shadow"
              title="Cross-project reports — the medulla box (no project)"
            >
              <Icon name="inbox" size={14} decorative className="text-ink-soft/80" />
              Medulla
            </button>
            <button
              type="button"
              data-role="bootstrap-new"
              onClick={onBootstrap}
              className="px-3 py-1.5 text-xs bg-bone text-ink border border-ink/15 rounded hover:shadow-contact transition-shadow"
              title="Read a new repo into its own brain"
            >
              + Read a new repo
            </button>
          </div>
        </div>

        {/* Presence strip (P1) — the team, at one glance, above the brains grid. */}
        <PresenceStrip
          presences={presence.presences}
          collisions={presence.collisions}
          error={presence.error}
        />

        {/* Grid */}
        <div
          ref={gridRef}
          role="list"
          aria-label="brains"
          tabIndex={0}
          onKeyDown={onGridKeyDown}
          className="flex-1 overflow-y-auto p-6 outline-none focus:ring-1 focus:ring-ink/10"
        >
          {error && (
            <div className="mb-4 rounded-lg border border-state-failure/25 bg-state-failure-tint/40 px-3 py-2 text-xs text-ink font-mono">
              {error}
            </div>
          )}
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            {cards.map((group) => {
              const entry = group.entry;
              const isSelf = entry.instance_id === selfId;
              // The viewing chip marks the ONE brain the tree is currently on
              // (§4A.8), now that per-brain Open (2H) lets it be a hosted brain.
              // Bound view (viewedRoot=null) → the self card; a hosted view → the
              // card whose project root matches. Exactly one card ever wears it.
              const viewing =
                viewedRoot == null
                  ? isSelf
                  : canonRootForCompare(brainProjectPath(entry)) === canonRootForCompare(viewedRoot);
              return (
                <BrainCard
                  key={entry.instance_id}
                  entry={entry}
                  isSelf={isSelf}
                  restSelector={restSelector}
                  knownNodeCount={isSelf && self ? self.graph_state.node_count : null}
                  knownEdgeCount={isSelf && self ? self.graph_state.edge_count : null}
                  selected={entry.instance_id === selectedId}
                  viewing={viewing}
                  duplicateWorkspace={group.duplicateWorkspace}
                  // Card-v2 GOLD renders on the OPEN/bound brain only (§4A.3.1);
                  // hosted brains pass null → the fields are absent-honest.
                  gold={isSelf ? { g1: v2.g1, g2: v2.g2, g3: v2.g3, g4: v2.g4 } : null}
                  onReread={reread}
                  onCalibrate={calibrate}
                  onSelect={(en) => setSelectedId(en.instance_id)}
                  onOpen={openBrain}
                  onOpenMailbox={openMailbox}
                />
              );
            })}
          </div>
          {cards.length === 0 && !error && (
            <div className="rounded-xl border border-ink/10 bg-bone/50 px-6 py-10 text-center text-sm text-ink-soft">
              No brains yet — read your first repo to begin.
            </div>
          )}
        </div>
      </div>

      {/* Receipt drawer (rung 0 of a card → its receipt) */}
      <BrainReceiptDrawer
        entry={selected}
        isSelf={selected?.instance_id === selfId}
        self={self}
        onClose={() => setSelectedId(null)}
        onOpen={openBrain}
        onSave={saveBrain}
        saving={selected != null && savingId === selected.instance_id}
        onDeleted={onDeleted}
        // Card-v2 DEPTH (§4A.3.1) — only for the OPEN/bound brain's receipt.
        depth={selected?.instance_id === selfId ? { d1: v2.d1, d2: v2.d2, calibrationReceipt: v2.g2?.receipt ?? null } : null}
      />

      {/* The Mailbox (§4A.11) — a drawer-class surface beside the grid. ESC returns. */}
      {mailboxFor && (
        <MailboxView
          brainRoot={mailboxFor.root}
          displayName={mailboxFor.name}
          onClose={() => setMailboxFor(null)}
        />
      )}

      {/* Owner alerts (honest doors) — the drawer the Landing's owner item lands on. */}
      {alertsOpen && (
        <OwnerAlertsPanel
          alerts={alerts}
          active={alertsActive}
          onAck={(id) => onAckAlerts([id])}
          onAckAll={onAckAlerts}
          onClose={() => setAlertsOpen(false)}
        />
      )}
    </div>
  );
}
