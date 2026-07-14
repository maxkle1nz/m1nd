/*
 * TheLanding — the unified human-gesture queue (HUMAN-VIEW-V2 F30 §2). One list,
 * every world plus the owner: merge_wait receipt stamps, block-candidate ratifies,
 * and owner alert acks. READS are aggregated here; every WRITE still travels its own
 * per-type verb — clicking an item only NAVIGATES to the existing flow (the tray card
 * for a stamp, the map for a ratify, the Hall for an owner alert). The badge counts
 * ALL queued gesture types and is labelled "await your hand" — never a bell (the
 * cockpit bell keeps its exact merge_wait-only semantics; two numbers, two names).
 */
import { buildLandingItems, type LandingItem, type UniverseResponse } from '../../lib/universe';

interface TheLandingProps {
  universe: UniverseResponse;
  /** Open a world's room (its map, where the tray + ratify live). */
  onOpenWorld: (root: string) => void;
  /** Open the owner room (the Hall) — where owner-scope detail lives. */
  onOpenOwner: () => void;
}

/** The small kind glyph — paper marks, never neon. Stamp = a receipt corner,
 *  ratify = a check, alert = an owner bell-less dot. Purely decorative. */
const KIND_MARK: Record<LandingItem['kind'], string> = {
  stamp: '▤',
  ratify: '✓',
  alert: '◈',
};

export default function TheLanding({ universe, onOpenWorld, onOpenOwner }: TheLandingProps) {
  const items = buildLandingItems(universe);
  const total = universe.totals.pending;

  return (
    <div
      data-role="the-landing"
      className="flex flex-col h-full w-72 shrink-0 border-l border-hairline bg-warm-paper/60"
    >
      <div className="flex items-center justify-between px-4 h-12 border-b border-hairline shrink-0">
        <span className="font-serif text-ink text-[15px]">the Landing</span>
        <span
          data-role="landing-badge"
          className={`text-xs font-mono px-2 py-0.5 rounded-full border ${
            total > 0
              ? 'text-ink border-verdict-reverify bg-verdict-reverify-tint'
              : 'text-ink-soft border-hairline'
          }`}
          title="every queued human gesture, across all worlds"
        >
          {total > 0 ? `${total} await your hand` : 'quiet'}
        </span>
      </div>

      {items.length === 0 ? (
        <div data-role="landing-empty" className="px-4 py-6 text-sm text-ink-soft leading-relaxed">
          Nothing awaits your hand. Every receipt is landed and every block is ratified.
        </div>
      ) : (
        <ul className="flex-1 overflow-y-auto py-1">
          {items.map((item) => {
            const isOwner = item.scope === 'owner';
            // A world item with no root can't open a room — routing it to the Hall
            // (the old `|| !item.worldRoot` fallback) landed the human in the WRONG
            // room silently. It renders DISABLED instead, the reason in its title.
            const missingRoot = !isOwner && !item.worldRoot;
            return (
            <li key={item.id}>
              <button
                type="button"
                data-role="landing-item"
                data-landing-kind={item.kind}
                data-landing-chip={item.chip}
                disabled={missingRoot}
                title={missingRoot ? 'world root unknown — refresh' : undefined}
                onClick={() => {
                  if (missingRoot) return;
                  if (isOwner) onOpenOwner();
                  else if (item.worldRoot) onOpenWorld(item.worldRoot);
                }}
                className={`w-full text-left px-4 py-2.5 flex items-start gap-2.5 transition-colors border-b border-hairline/50 ${
                  missingRoot ? 'opacity-50 cursor-not-allowed' : 'hover:bg-porcelain/70'
                }`}
              >
                <span
                  aria-hidden
                  className="mt-0.5 text-verdict-reverify font-mono text-sm select-none"
                >
                  {KIND_MARK[item.kind]}
                </span>
                <span className="flex-1 min-w-0">
                  <span className="flex items-center gap-1.5">
                    <span
                      data-role="landing-item-chip"
                      className={`text-[11px] font-mono px-1.5 py-px rounded truncate max-w-[9rem] ${
                        item.scope === 'owner'
                          ? 'bg-bone text-ink-soft border border-hairline'
                          : 'bg-porcelain text-ink border border-hairline'
                      }`}
                      title={item.scope === 'owner' ? 'owner scope' : item.chip}
                    >
                      {item.chip}
                    </span>
                  </span>
                  <span className="block text-[13px] text-ink-soft mt-1 leading-snug">
                    {item.line}
                  </span>
                </span>
              </button>
            </li>
            );
          })}
        </ul>
      )}

      <div className="px-4 py-2 text-[11px] text-ink-soft/70 border-t border-hairline shrink-0 leading-snug">
        Reads gathered here; each hand lands through its own door.
      </div>
    </div>
  );
}
