/*
 * MissionTray — the fixed write-mode surface (HUMAN-VIEW-V2 F2.5 §3). A right-edge
 * tray, collapsible, present on EVERY surface (mounted in the App shell, outside the
 * surface switch). Collapsed: a thin vertical strip with per-phase counts. Expanded
 * (~320px): the mission cards, `failed` PINNED atop (§3c), read-only over letters
 * (§3e). This component is pure/presentational — the App wires `useMissions` + the
 * localStorage dismiss/expand state (MissionTrayLive) — so the honesty at the pixel
 * boundary is unit-provable with a captured-fixture SSR render.
 *
 * Degraded states are honest, never a broken shell: a pre-F2.5a owner (no
 * `kind=mission` read) says "mission letters need an updated owner"; a read error
 * says so; the first fetch shows a discreet "reading…"; an empty ready tray invites
 * "no missions yet — point an agent at a block" (§3d). Copy law throughout.
 */
import type { MissionHead } from '../../lib/missions';
import { PHASE_META, PHASES, phaseCounts, phaseLabel, sortForTray } from '../../lib/missions';
import type { MissionsStatus } from '../../hooks/useMissions';
import MissionCard from './MissionCard';

export interface MissionTrayProps {
  missions: MissionHead[];
  status: MissionsStatus;
  error?: string | null;
  expanded: boolean;
  onToggleExpanded: () => void;
  /** `failed` mission_ids the human dismissed (§3c) — un-pinned, presentation-only. */
  dismissedIds?: Set<string>;
  onDismiss?: (missionId: string) => void;
  /** `landed` mission_ids whose provenance chain is open (§3f). */
  provenanceIds?: Set<string>;
  onToggleProvenance?: (missionId: string) => void;
  onOpenBlock: (blockId: string) => void;
  /** Injectable clock for a deterministic elapsed under test. */
  now?: number;
}

export default function MissionTray({
  missions,
  status,
  error = null,
  expanded,
  onToggleExpanded,
  dismissedIds,
  onDismiss,
  provenanceIds,
  onToggleProvenance,
  onOpenBlock,
  now,
}: MissionTrayProps) {
  const dismissed = dismissedIds ?? new Set<string>();
  const counts = phaseCounts(missions);
  const total = missions.length;

  // The list: failed pinned atop (§3c), then newest first; a dismissed failure is
  // un-pinned (hidden from the list) but its data is untouched in the box.
  const ordered = sortForTray(missions).filter(
    (h) => !(h.head.phase === 'failed' && dismissed.has(h.mission_id)),
  );

  // Collapsed — the thin vertical strip with per-phase counts (§3a).
  if (!expanded) {
    return (
      <div
        data-role="mission-tray"
        data-expanded="false"
        className="fixed top-12 right-0 bottom-0 z-30 w-9 flex flex-col items-center border-l border-hairline bg-porcelain/95 py-2 gap-2"
      >
        <button
          type="button"
          data-role="mission-tray-toggle"
          onClick={onToggleExpanded}
          aria-label="expand the mission tray"
          title="Missions"
          className="text-[10px] font-mono text-ink-soft hover:text-ink [writing-mode:vertical-rl] rotate-180 tracking-wide"
        >
          missions{total > 0 ? ` ${total}` : ''}
        </button>
        <div className="flex flex-col items-center gap-1.5 mt-1">
          {PHASES.filter((p) => counts[p] > 0).map((p) => (
            <span
              key={p}
              data-role="mission-strip-count"
              data-phase={p}
              title={`${counts[p]} ${phaseLabel(p)}`}
              className="flex flex-col items-center text-[10px] font-mono text-ink-soft leading-none"
            >
              <span aria-hidden className="text-ink">
                {PHASE_META[p].glyph}
              </span>
              {counts[p]}
            </span>
          ))}
        </div>
      </div>
    );
  }

  // Expanded — the mission cards (~320px).
  return (
    <div
      data-role="mission-tray"
      data-expanded="true"
      className="fixed top-12 right-0 bottom-0 z-30 w-80 flex flex-col border-l border-hairline bg-porcelain/97 shadow-card"
    >
      <div className="flex items-center gap-2 px-3 py-2 border-b border-ink/10 shrink-0">
        <span className="text-sm font-semibold text-ink">Missions</span>
        {total > 0 && <span className="font-mono text-[10px] text-ink-soft">{total}</span>}
        <button
          type="button"
          data-role="mission-tray-toggle"
          onClick={onToggleExpanded}
          aria-label="collapse the mission tray"
          title="collapse"
          className="ml-auto text-ink-soft hover:text-ink text-sm px-1"
        >
          ›
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-2 space-y-2">
        {status === 'unsupported' ? (
          <div data-role="mission-tray-unsupported" className="text-xs text-ink-soft px-1 py-2 leading-relaxed">
            mission letters need an updated owner — this owner does not serve the mission read yet.
          </div>
        ) : status === 'error' ? (
          <div data-role="mission-tray-error" className="text-xs text-state-failure px-1 py-2 leading-relaxed break-words">
            couldn’t read mission letters{error ? ` — ${error}` : ''}
          </div>
        ) : status === 'loading' && total === 0 ? (
          <div data-role="mission-tray-loading" className="text-xs text-ink-soft px-1 py-2">
            reading…
          </div>
        ) : ordered.length === 0 ? (
          <div data-role="mission-tray-empty" className="text-xs text-ink-soft px-1 py-2 leading-relaxed">
            no missions yet — point an agent at a block.
          </div>
        ) : (
          ordered.map((head) => (
            <MissionCard
              key={head.mission_id}
              head={head}
              onOpenBlock={onOpenBlock}
              onDismiss={onDismiss}
              provenanceOpen={provenanceIds?.has(head.mission_id) ?? false}
              onToggleProvenance={onToggleProvenance}
              now={now}
            />
          ))
        )}
      </div>

      <div className="shrink-0 border-t border-ink/10 px-3 py-1.5 text-[10px] font-mono text-ink-soft" data-role="mission-tray-note">
        read-only over letters — the tray shows, the compose posts.
      </div>
    </div>
  );
}
