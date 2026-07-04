/*
 * OrientationBeats — the north packet rendered humanly (HUMAN-LAYER-PRD §4A.2).
 *
 * Three quiet callouts, NOT a spotlight tour: the map (file/connection counts),
 * what matters (top anchors), and the honest gaps (the violet gap card). Each beat
 * is one sentence + one dismiss; ESC dismisses all, forever (INV-12). Renders only
 * at zero brains and never returns once dismissed — the parent gates mounting; this
 * component owns per-beat + ESC-forever persistence.
 *
 * The only violet on the surface is the reused GapCard (abstain allow-list) — this
 * file names no violet itself, so the quarantine stays green.
 */
import { useEffect, useState } from 'react';
import GapCard from '../soft/GapCard';
import {
  orientationBeats,
  beatDismissed,
  dismissBeat,
  dismissOrientationForever,
  allBeatsDismissed,
  type KV,
  type OrientationBeat,
} from '../../lib/threshold';

interface OrientationBeatsProps {
  kv: KV;
  nodeCount: number | null;
  edgeCount: number | null;
  anchorLabels: string[];
  memoryCount: number | null;
  /** The honest gaps from north (rendered as the violet gap card for the gaps beat). */
  gaps: string[];
  /** Called when every beat is gone (individually or via ESC) so the parent unmounts. */
  onSpent: () => void;
}

export default function OrientationBeats({
  kv,
  nodeCount,
  edgeCount,
  anchorLabels,
  memoryCount,
  gaps,
  onSpent,
}: OrientationBeatsProps) {
  // Local tick to re-render on dismissal (persistence is the source of truth).
  const [, setTick] = useState(0);
  const bump = () => setTick((t) => t + 1);

  // ESC dismisses ALL, forever (INV-12).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        dismissOrientationForever(kv);
        onSpent();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [kv, onSpent]);

  // Already spent (e.g. a returning render) → nothing.
  if (allBeatsDismissed(kv)) return null;

  const beats = orientationBeats({ nodeCount, edgeCount, anchorLabels, memoryCount });
  const visible = beats.filter((b) => !beatDismissed(kv, b.beat));
  if (visible.length === 0) {
    onSpent();
    return null;
  }

  const dismissOne = (beat: OrientationBeat) => {
    dismissBeat(kv, beat);
    if (allBeatsDismissed(kv)) onSpent();
    else bump();
  };

  return (
    <div
      data-role="orientation"
      className="pointer-events-none fixed inset-0 z-30 flex flex-col items-center justify-end pb-8 gap-3"
    >
      {visible.map((b) => (
        <div
          key={b.beat}
          data-role="orientation-beat"
          data-beat={b.beat}
          className="pointer-events-auto w-full max-w-md mx-4 rounded-lg border border-ink/12 bg-porcelain shadow-card px-4 py-3"
        >
          {b.beat === 'gaps' && gaps.length > 0 ? (
            <div className="space-y-2">
              <div className="text-[11px] uppercase tracking-wide text-ink-soft/70">what I don't know yet</div>
              <GapCard gap={gaps[0]} />
              {gaps.length > 1 && (
                <div className="text-[10px] text-ink-soft/70">and {gaps.length - 1} more</div>
              )}
            </div>
          ) : (
            <div className="text-sm text-ink">{b.text}</div>
          )}
          <div className="mt-2 flex justify-end">
            <button
              type="button"
              data-role="dismiss-beat"
              onClick={() => dismissOne(b.beat)}
              className="text-[11px] text-ink-soft hover:text-ink font-mono"
            >
              got it
            </button>
          </div>
        </div>
      ))}
      <div className="pointer-events-auto text-[10px] text-ink-soft/60 font-mono">esc dismisses all · never shown again</div>
    </div>
  );
}
