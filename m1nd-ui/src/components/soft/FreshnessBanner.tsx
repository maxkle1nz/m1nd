/*
 * FreshnessBanner — honest degraded / cold-state banner (HUMAN-LAYER-PRD §3.5, §4.3).
 * Violet-outlined, calm (terracotta for degraded binding, never alarm red).
 * On the violet-lint allow-list. Abstain-class: never animates.
 */

interface FreshnessBannerProps {
  message: string;
  tone?: 'unknown' | 'degraded';
  action?: { label: string; onClick: () => void };
}

export default function FreshnessBanner({ message, tone = 'unknown', action }: FreshnessBannerProps) {
  const isUnknown = tone === 'unknown';
  return (
    <div
      data-abstain={isUnknown ? 'true' : undefined}
      data-role="freshness-banner"
      className={
        'flex items-center justify-between gap-3 rounded-md px-3 py-2 text-sm ' +
        (isUnknown
          ? 'border border-iris/40 bg-iris-veil/50 text-iris-deep'
          : 'border border-verdict-reverify/50 bg-verdict-reverify-tint text-ink')
      }
    >
      <span className="leading-snug">{message}</span>
      {action && (
        <button
          type="button"
          onClick={action.onClick}
          className="shrink-0 text-[11px] font-medium px-2 py-1 rounded bg-bone text-ink border border-ink/15 hover:shadow-contact transition-shadow"
        >
          {action.label}
        </button>
      )}
    </div>
  );
}
