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
import { useState } from 'react';
import type { StateCounts } from '../../lib/buildMap';
import { Icon } from '../../lib/icons/registry';

/** The reconcile truth the Unmapped tray renders (F3b §B). `reconciled:false` is the
 *  neutral "never reconciled" absence — never a false zero. */
export interface UnmappedReport {
  reconciled: boolean;
  total: number;
  files: string[];
}

export interface SystemHealthSidebarProps {
  counts: StateCounts;
  /** The real unmapped from the reconcile (Slice 3) — replaces the declared residue. */
  unmapped: UnmappedReport;
  lastScan?: string | null;
  onOpenTree?: () => void;
  /** Test seam (mirrors BuildMap's `initialSelectedId`): force the tray open so the
   *  expanded list renders under `renderToStaticMarkup` (no click in SSR). */
  initialUnmappedExpanded?: boolean;
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

export default function SystemHealthSidebar({
  counts,
  unmapped,
  onOpenTree,
  initialUnmappedExpanded = false,
}: SystemHealthSidebarProps) {
  const [expanded, setExpanded] = useState(initialUnmappedExpanded);
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

      {/* The permanent Unmapped tray (F7 — never hidden, present even at 0). Shows
          the REAL reconcile unmapped (`unmapped_total`), not the declared residue.
          A never-reconciled store says so honestly (neutral absence, not a false 0);
          a reconciled store's count is clickable to expand the materialized list. */}
      <div className="px-4 py-3 border-t border-ink/10 mt-auto" data-role="unmapped-tray">
        <div className="text-[10px] uppercase tracking-wide text-ink-soft">Unmapped</div>
        {!unmapped.reconciled ? (
          <>
            <div className="text-xs text-ink-soft mt-1 font-mono" data-role="unmapped-unreconciled">
              not reconciled yet — run reconcile
            </div>
            <p className="text-[10px] text-ink-soft mt-1">
              the real unmapped is unknown until the map is reconciled against the repo.
            </p>
          </>
        ) : (
          <>
            <button
              type="button"
              data-role="unmapped-toggle"
              aria-expanded={expanded}
              onClick={() => setExpanded((v) => !v)}
              disabled={unmapped.total === 0}
              className="mt-1 w-full flex items-center justify-between text-xs text-ink font-mono hover:text-ink disabled:cursor-default group"
              title={unmapped.total === 0 ? 'nothing unmapped' : expanded ? 'collapse the list' : 'show the files'}
            >
              <span>
                <span className="tabular-nums" data-role="unmapped-count">{unmapped.total}</span>{' '}
                {unmapped.total === 1 ? 'file' : 'files'}
              </span>
              {unmapped.total > 0 && (
                <span className="text-ink-soft text-[10px]">{expanded ? '▾' : '▸'}</span>
              )}
            </button>
            {expanded && unmapped.total > 0 && (
              <div className="mt-1.5" data-role="unmapped-list">
                <ul className="max-h-40 overflow-y-auto space-y-0.5 border border-ink/10 rounded bg-bone/40 p-1.5">
                  {unmapped.files.map((f) => (
                    <li key={f} className="text-[10px] font-mono text-ink-soft break-all">
                      {f}
                    </li>
                  ))}
                </ul>
                {unmapped.files.length < unmapped.total && (
                  <p className="text-[10px] text-ink-soft mt-1" data-role="unmapped-cap">
                    showing {unmapped.files.length} of {unmapped.total} — the cap is honest, the count is true.
                  </p>
                )}
              </div>
            )}
            <p className="text-[10px] text-ink-soft mt-1">files claimed by no block — the map never pretends full coverage.</p>
          </>
        )}
      </div>
    </aside>
  );
}
