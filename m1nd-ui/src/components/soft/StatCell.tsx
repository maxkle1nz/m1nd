/*
 * StatCell / StatValue — the precision primitives (HUMAN-LAYER-PRD §4A.7, INV-13).
 *
 * "Precision is mechanical." Every count/age/score renders right-aligned in
 * tabular mono (IBM Plex Mono is tabular by construction; `tabular-nums` is the
 * belt). One stats ROW is: icon (14 px, 1.5 stroke, currentColor) · label ·
 * VALUE right-aligned. The icon always carries a text label on its first use per
 * surface (the visible `label` here satisfies INV-13, so the icon is decorative).
 *
 * These components are the ONLY sanctioned way to render a fact row on a card
 * face or a group header — so the "right-aligned tabular mono" rule is one class
 * assertion away in the tests, not scattered Tailwind.
 */
import { Icon, type IconName } from '../../lib/icons/registry';

/** A bare numeric/age/score value: tabular mono, right-aligned. */
export function StatValue({ children, className = '' }: { children: React.ReactNode; className?: string }) {
  return (
    <span
      data-role="stat-value"
      className={`font-mono tabular-nums text-right ${className}`}
    >
      {children}
    </span>
  );
}

export interface StatCellProps {
  /** Concept icon (registry name) — always paired with the visible label. */
  icon: IconName;
  /** The visible text label (satisfies INV-13's label-on-first-use). */
  label: string;
  /** The right-aligned value (count/age/score). Omit for a label-only row. */
  value?: React.ReactNode;
  /** Extra classes on the row container. */
  className?: string;
  /** data-role for targeted tests. */
  role?: string;
  title?: string;
}

/**
 * One stats row, strict order (§4A.7 card slots #3): icon · label · value(right).
 * fact-row height is owned by the parent's density (24 compact / 28 comfortable).
 */
export function StatCell({ icon, label, value, className = '', role, title }: StatCellProps) {
  return (
    <div
      data-role={role ?? 'stat-cell'}
      title={title}
      className={`flex items-center gap-1.5 text-[11px] text-ink-soft ${className}`}
    >
      <Icon name={icon} size={14} decorative className="shrink-0 text-ink-soft/80" />
      <span className="min-w-0 truncate">{label}</span>
      {value != null && <StatValue className="ml-auto text-ink">{value}</StatValue>}
    </div>
  );
}
