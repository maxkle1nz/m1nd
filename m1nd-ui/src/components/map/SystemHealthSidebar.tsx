/*
 * SystemHealthSidebar — the left rail (HUMAN-VIEW-V2-SCREENS §1.1; PRD F5/F7).
 *
 * Counts by state (derived from the rollup, never re-computed here), the runtime
 * axis rendered as an honest "no runtime signal" (F0-TECH §6 — the render never
 * calls the mutating overlay verb; absence is not a fabricated blue), and the
 * permanent Unmapped tray (F7 — the map never pretends full coverage, so the tray
 * is present even at 0 files). The Living Tree stays one click away (PRD: the
 * deterministic surface is never killed).
 */
import type { StateCounts } from '../../lib/buildMap';
import { Icon } from '../../lib/icons/registry';

export interface SystemHealthSidebarProps {
  counts: StateCounts;
  unmappedCount: number;
  lastScan?: string | null;
  onOpenTree?: () => void;
}

/** One counted state row: a dot + a word + the number (colorblind-redundant). */
function StateRow({ dot, label, count, role }: { dot: string; label: string; count: number; role: string }) {
  return (
    <div className="flex items-center justify-between text-xs" data-role={role}>
      <span className="flex items-center gap-1.5 text-ink">
        <span className={`w-2 h-2 rounded-full inline-block ${dot}`} aria-hidden />
        {label}
      </span>
      <span className="font-mono tabular-nums text-ink-soft">{count}</span>
    </div>
  );
}

export default function SystemHealthSidebar({ counts, unmappedCount, onOpenTree }: SystemHealthSidebarProps) {
  return (
    <aside className="w-52 shrink-0 border-r border-ink/10 bg-porcelain flex flex-col overflow-y-auto" data-role="system-health">
      {/* Surface identity + nav (the Living Tree is one click away). */}
      <div className="px-4 py-3 border-b border-ink/10">
        <div className="flex items-center gap-1.5 text-ink font-semibold text-sm">
          <Icon name="blocks" size={16} decorative />
          Build Map
        </div>
        <button
          type="button"
          data-role="open-tree"
          onClick={onOpenTree}
          className="mt-2 w-full flex items-center gap-1.5 px-2 py-1 text-xs text-ink-soft hover:text-ink hover:bg-bone/60 rounded transition-colors"
          title="Open the Living Tree (the deterministic navigation surface)"
        >
          <Icon name="layer" size={14} decorative />
          Living Tree
        </button>
      </div>

      {/* System Health counts. */}
      <div className="px-4 py-3 space-y-1.5">
        <div className="text-[10px] uppercase tracking-wide text-ink-soft mb-1">System Health</div>
        <StateRow dot="bg-verdict-act" label="Evidence-backed" count={counts['evidence-backed']} role="count-evidence" />
        <StateRow dot="bg-verdict-reverify" label="Needs evidence" count={counts['needs-evidence']} role="count-needs" />
        <StateRow dot="bg-state-failure" label="Broken" count={counts.broken} role="count-broken" />
        <StateRow dot="bg-state-unverified" label="Unknown" count={counts.unknown} role="count-unknown" />
        <StateRow dot="bg-bone border border-ink/20" label="Planned" count={counts.planned} role="count-planned" />
        <div className="flex items-center justify-between text-xs pt-0.5" data-role="runtime-axis">
          <span className="flex items-center gap-1.5 text-ink">
            <span className="w-2 h-2 rounded-full inline-block bg-socket-blue/40" aria-hidden />
            Runtime
          </span>
          <span className="font-mono text-ink-soft text-[11px]">no signal</span>
        </div>
      </div>

      {/* The permanent Unmapped tray (F7). */}
      <div className="px-4 py-3 border-t border-ink/10 mt-auto" data-role="unmapped-tray">
        <div className="text-[10px] uppercase tracking-wide text-ink-soft">Unmapped</div>
        <div className="text-xs text-ink mt-1 font-mono">
          <span className="tabular-nums">{unmappedCount}</span> {unmappedCount === 1 ? 'file' : 'files'}
        </div>
        <p className="text-[10px] text-ink-soft mt-1">files claimed by no block — the map never pretends full coverage.</p>
      </div>
    </aside>
  );
}
