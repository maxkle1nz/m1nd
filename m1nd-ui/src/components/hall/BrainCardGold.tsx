/*
 * BrainCardGold — the card-v2 GOLD fields for the OPEN brain (HUMAN-LAYER-PRD
 * §4A.3.1). Four calm stats rows on the bound brain's face: G1 freshness-vs-git
 * (+ [Re-read]), G2 calibration chip (+ [Calibrate once]), G3 the compounding
 * meter, G4 aliveness. Each is one line (icon · label · value), tabular (INV-13).
 *
 * A hosted brain shows these ABSENT-honest (this component renders only for the
 * open/bound brain — the caller gates on isSelf; today-vs-2H table). Anti-scope:
 * no timeseries, no aggregate score, no animated percentages.
 */
import { StatCell } from '../soft/StatCell';
import { Icon } from '../../lib/icons/registry';
import type { FreshnessG1, CalibrationG2, CompoundingG3, AlivenessG4 } from '../../lib/cardV2';

interface BrainCardGoldProps {
  g1: FreshnessG1 | null;
  g2: CalibrationG2 | null;
  g3: CompoundingG3 | null;
  g4: AlivenessG4 | null;
  onReread?: () => void;
  onCalibrate?: () => void;
}

export default function BrainCardGold({ g1, g2, g3, g4, onReread, onCalibrate }: BrainCardGoldProps) {
  return (
    <div className="mt-3 space-y-1 border-t border-ink/5 pt-2" data-role="card-gold">
      {/* G1 — freshness vs git */}
      {g1 && (
        <div className="flex items-center gap-2">
          <StatCell
            icon="freshness"
            label={g1.caption}
            role="g1-freshness"
            className="flex-1"
          />
          {!g1.allCurrent && onReread && (
            <button
              type="button"
              data-role="reread"
              onClick={(e) => {
                e.stopPropagation();
                onReread();
              }}
              className="inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded border border-ink/15 text-ink-soft hover:text-ink hover:shadow-contact shrink-0"
              title="re-read this repo (same root)"
            >
              <Icon name="ingest" size={14} decorative /> Re-read
            </button>
          )}
        </div>
      )}

      {/* G2 — calibration chip */}
      {g2 && (
        <div className="flex items-center gap-2">
          <StatCell icon="calibration" label={g2.caption} role="g2-calibration" className="flex-1" title={g2.receipt ?? undefined} />
          {!g2.measured && onCalibrate && (
            <button
              type="button"
              data-role="calibrate"
              onClick={(e) => {
                e.stopPropagation();
                onCalibrate();
              }}
              className="inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded border border-ink/15 text-ink-soft hover:text-ink hover:shadow-contact shrink-0"
              title="measure calibration on this repo once"
            >
              <Icon name="calibration" size={14} decorative /> Calibrate once
            </button>
          )}
        </div>
      )}

      {/* G3 — the compounding meter */}
      {g3 && <StatCell icon="memory" label={g3.caption} role="g3-compounding" />}

      {/* G4 — aliveness */}
      {g4 && <StatCell icon="agents" label={g4.caption} role="g4-aliveness" />}
    </div>
  );
}
