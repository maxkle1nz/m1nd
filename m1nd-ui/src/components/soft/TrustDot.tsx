/*
 * TrustDot — one calm band color per node (HUMAN-LAYER-PRD §3.2).
 * On the violet-lint allow-list: insufficient_evidence renders IRIS violet.
 * Fixed size; abstain (unknown) never animates (INV-02).
 */
import { BAND_STYLE, type TrustBand } from '../../lib/softProof';

interface TrustDotProps {
  band: TrustBand;
  /** Slow tremor breath when the file is churning now (§3.2). Never on unknown. */
  breathing?: boolean;
  title?: string;
}

export default function TrustDot({ band, breathing = false, title }: TrustDotProps) {
  const style = BAND_STYLE[band];
  // The honest-unknown dot is abstain-class: it must never carry the breath.
  const canBreathe = breathing && !style.isUnknown;
  return (
    <span
      role="img"
      aria-label={`trust: ${style.label}`}
      title={title ?? style.label}
      data-band={band}
      data-abstain={style.isUnknown ? 'true' : undefined}
      className={
        'inline-block w-2 h-2 rounded-full shrink-0' +
        (style.isUnknown ? ' ring-1 ring-iris/50' : '') +
        (canBreathe ? ' tremor-breath' : '')
      }
      style={{ backgroundColor: style.color }}
    />
  );
}
