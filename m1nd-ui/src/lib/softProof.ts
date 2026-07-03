/*
 * SOFT PROOF semantics — the honesty rules made into code (HUMAN-LAYER-PRD §2, §3, §7).
 *
 * This module is deliberately the ONE non-component place allowed to name the
 * violet (`iris`) token family (see scripts/violet-lint.mjs allow-list): it is
 * where the abstain/unknown → violet mapping is decided. Components consume the
 * band/state enums it returns and never hard-code violet themselves except the
 * abstain-class components on the lint allow-list.
 */

// ── Trust band → calm dot color (PRD §3.2) ────────────────────────────────────
// trust_band() in m1nd-core/src/trust.rs:78 maps tiers to these tokens; the
// TrustTier enum serializes as PascalCase variant names (trust.rs:59, no rename).

export type TrustBand = 'low' | 'medium' | 'high' | 'insufficient_evidence';

/** Map a serialized TrustTier ("LowRisk"/"MediumRisk"/"HighRisk"/"Unknown") to a band. */
export function tierToBand(tier: string): TrustBand {
  switch (tier) {
    case 'LowRisk':
      return 'low';
    case 'MediumRisk':
      return 'medium';
    case 'HighRisk':
      return 'high';
    case 'Unknown':
    default:
      return 'insufficient_evidence';
  }
}

export interface BandStyle {
  /** CSS color for the dot. Violet ONLY for insufficient_evidence. */
  color: string;
  /** Plain, non-epistemic label for tooltips at rung 0–1. */
  label: string;
  /** True only for the honest-unknown band — abstain-class, never animates. */
  isUnknown: boolean;
}

// Values come from CSS variables at render time; these are the fallbacks used in
// jsdom tests where getComputedStyle does not resolve custom properties.
export const BAND_STYLE: Record<TrustBand, BandStyle> = {
  low: { color: 'var(--verdict-act, #6fa287)', label: 'low risk', isUnknown: false },
  medium: { color: 'var(--verdict-reverify, #c89b3c)', label: 'worth a second look', isUnknown: false },
  high: { color: 'var(--state-failure, #b0563b)', label: 'high risk', isUnknown: false },
  insufficient_evidence: {
    // IRIS — the brand color, reserved for the honest unknown.
    color: 'var(--verdict-abstain, #7c3aed)',
    label: "no evidence seen either way yet",
    isUnknown: true,
  },
};

// ── Post-it states (PRD §3.3) ─────────────────────────────────────────────────

export type PostItState = 'fresh' | 'aging' | 'stale' | 'unknown';

/** The 30-day rule `north` applies (server.rs:3038-3054), mirrored client-side. */
export const STALE_AFTER_MS = 30 * 24 * 60 * 60 * 1000;

export interface PostItAge {
  /** ms since authored, or null when provenance is absent (never faked). */
  ageMs: number | null;
  /** source agent, or null when absent (never faked). */
  sourceAgent: string | null;
  /** engine/evidence flagged this claim's cited code as changed. */
  evidenceStale?: boolean;
}

/**
 * Resolve the visual state of a post-it (PRD §3.3 table).
 *  - unknown  : provenance absent (no created/source_agent) → violet-outlined.
 *  - stale    : evidence drift OR anchored file flagged changed → flipped face-down.
 *  - aging    : age ≥ 30d, not re-verified → curled corner.
 *  - fresh    : age < 30d → flat bone tag.
 * Provenance-absent (unknown) takes precedence: we cannot claim freshness we can't prove.
 */
export function postItState(age: PostItAge): PostItState {
  if (age.ageMs == null && age.sourceAgent == null) return 'unknown';
  if (age.evidenceStale) return 'stale';
  if (age.ageMs != null && age.ageMs >= STALE_AFTER_MS) return 'aging';
  return 'fresh';
}

/** Humanize an age in ms to a caption; null → the honest "age unknown". */
export function humanizeAge(ageMs: number | null): string {
  if (ageMs == null) return 'age unknown';
  const s = Math.round(ageMs / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.round(h / 24);
  return `${d}d ago`;
}

/** The author chip label — never faked (PRD §3.3, INV-04). */
export function authorLabel(sourceAgent: string | null): string {
  return sourceAgent ?? 'author unknown';
}

// ── Floor-not-ceiling language (PRD INV-08) ───────────────────────────────────

/**
 * Render a blast/reach count. When the result is truncated or the subgraph is
 * incomplete, the count is a FLOOR: "≥ N mapped …". A crisp bare integer is only
 * allowed when the engine reports untruncated coverage.
 */
export function blastCountPhrase(total: number, truncated: boolean, noun = 'mapped files'): string {
  return truncated ? `≥ ${total} ${noun}` : `${total} ${noun}`;
}

// ── Action-language map (PRD §2, INV-07) ──────────────────────────────────────
// Uncertainty renders as NEXT-ACTION guidance, never raw epistemology, at rung 0–1.
// Each entry is a shipped constant (unit-tested), sourced from real engine fields.
// The `banned` vocabulary must NOT appear in any rung-0–1 rendering.

export const RUNG01_BANNED_VOCABULARY = [
  'binding',
  'calibration',
  'calibrated',
  'closure',
  'abstain',
  'conformal',
  'provenance',
  'epistemic',
  'insufficient_evidence',
] as const;

export interface ActionCopy {
  /** Plain, calm, action-first line for the vibecoder (rung 0–1). */
  text: string;
  /** Exactly one suggested next step, or null when none applies. */
  action: string | null;
}

/**
 * Translate an engine gap/verdict string into action language + one next step.
 * Falls back to the generic "double-check" affordance when no specific mapping
 * or next-step field is present (INV-07: never a bare warning).
 */
export function actionLanguageForGap(gap: string): ActionCopy {
  const g = gap.toLowerCase();
  if (g.includes('empty or unbound') || g.includes('needs_ingest')) {
    return { text: "I haven't read this repo yet.", action: 'Read the repo' };
  }
  if (g.includes('not full trust') || g.includes('orientation only')) {
    return { text: 'Let me double-check this first.', action: null };
  }
  if (g.includes('no durable memory') || g.includes('neither boot_memory')) {
    return { text: "I haven't studied this yet — one read fixes that.", action: 'Read it' };
  }
  if (g.includes('changed since') || g.includes('re-read')) {
    return { text: 'This changed since I last read it — re-reading before we edit.', action: 'Re-read' };
  }
  // Generic honest fallback — still an action, never a bare warning.
  return { text: 'Worth a second look before we rely on this.', action: 'Double-check' };
}

/** The rung-0–1 rendering of a north `next_move` — surfaced as the primary button. */
export function nextMoveButtonLabel(nextMove: string | null | undefined): string {
  if (!nextMove) return 'Double-check';
  if (/ingest/i.test(nextMove)) return 'Read the repo';
  if (/surgical_context|surgical/i.test(nextMove)) return 'Ground it first';
  if (/calibrate/i.test(nextMove)) return 'Calibrate once';
  return 'Do the next step';
}
