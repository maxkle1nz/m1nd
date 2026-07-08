/*
 * The icon registry — the ONE import site for `lucide-react` (HUMAN-LAYER-PRD §4A.7).
 *
 * Founder direction: "com ícones certos para cada coisa para fácil visualização —
 * nerd adora iconezinho." The ask is PRECISION, not decoration. So there is exactly
 * one icon per concept, every concept always the same icon, and this module is the
 * only file allowed to `import … from 'lucide-react'` — the icon-lint
 * (scripts/icon-lint.mjs) fails any direct lucide import anywhere else, so
 * "one icon per concept" is greppable and cannot drift.
 *
 * Precision rules encoded here (INV-13):
 *  - stroke 1.5 universal (hairline-adjacent, sits next to IBM Plex Mono);
 *  - `currentColor` always — icons inherit ink/ink-soft, so the §6.2 violet
 *    quarantine covers them for free (an icon can only wear iris inside an
 *    abstain-class component that already sets the color);
 *  - 14 px inline (chips, stats) · 16 px in headers/actions — the two sanctioned
 *    sizes, `IconSize`.
 *
 * The BANNED `Sparkles` (and kin) is deliberately NOT re-exported and the lint
 * bans it outright: "AI glitter" is a promise, not a fact — the seek meaning-mode
 * is a labeled text toggle, its honesty markers are the sufficiency line and the
 * verdict chip, never a magic star (§4A.7).
 *
 * License: lucide-react is ISC (Feather/MIT heritage) — vendored text in
 * ./LICENSE-lucide.txt, exactly as the fonts vendor theirs (§6.5).
 */
import {
  Waypoints,
  Spline,
  History,
  Tag,
  Ruler,
  Check,
  RotateCcw,
  CircleDashed,
  CircleOff,
  Cable,
  Inbox,
  RefreshCw,
  ReceiptText,
  Trash2,
  Eye,
  Search,
  Filter,
  Layers,
  FolderTree,
  Shapes,
  FileCode,
  SquareFunction,
  Box,
  Boxes,
  FileText,
  type LucideIcon,
} from 'lucide-react';

/**
 * CONCEPT → ICON (binding; one icon per concept, no synonyms — §4A.7 table).
 * The keys are the concept names; components reference an icon ONLY through this
 * map (or the {@link Icon} component below), never by importing lucide directly.
 */
export const ICON = {
  // Graph / structure
  graph: Waypoints, // nodes ("2,089 nodes"), receipt fingerprint
  edges: Spline, // edges ("7,323 edges"), map legend
  blocks: Boxes, // the Build Map surface + a SystemBlock (Human View v2, §0)
  // Freshness / memory / calibration
  freshness: History, // G1 caption, freshness banner, stale post-it back
  memory: Tag, // post-it chips, memory counts, G3 — the specimen-tag motif
  calibration: Ruler, // G2 chip, calibration line
  // Verdict glyphs (inside VerdictChip, before the text)
  verdictAct: Check,
  verdictReverify: RotateCcw,
  verdictAbstain: CircleDashed, // supersedes the plain iris dot; iris ink; NEVER animates
  verdictUnprovable: CircleOff, // the 4th rung: nothing exists to check against; grey ink; NEVER animates
  // Aliveness / inbox / ingest
  agents: Cable, // G4 caption, receipt sessions — the `--attach` word made visible
  inbox: Inbox, // D3 count + receipt list (before the verbs exist: never shown)
  ingest: RefreshCw, // [Re-read] actions, Threshold's first action (never an ambient spinner)
  receipt: ReceiptText, // "more in the receipt →" affordances
  delete: Trash2, // ONLY inside the §4A.4 forget flow (step 1 card) — never a card face
  viewing: Eye, // the viewing chip on the open brain + the Brain Chip
  // Reading-the-Tree instruments (§4A.10)
  search: Search, // the search field (both modes)
  filter: Filter, // the filter bar toggle
  layer: Layers, // group-by-layer mode + group headers
  groupDir: FolderTree, // group-by-directory mode picker
  groupKind: Shapes, // group-by-kind mode picker
  // KIND glyphs (group headers + filter chips only, never per-row)
  kindFile: FileCode,
  kindFunction: SquareFunction,
  kindStruct: Box, // struct / class
  kindDoc: FileText,
  kindMemory: Tag, // same concept, same icon — the law holds across tables
} as const satisfies Record<string, LucideIcon>;

export type IconName = keyof typeof ICON;

/** The two sanctioned inline sizes (§4A.7): 14 px inline, 16 px in headers/actions. */
export type IconSize = 14 | 16;

/** Universal stroke — hairline-adjacent, sits correctly next to IBM Plex Mono. */
export const ICON_STROKE = 1.5;

export interface IconProps {
  name: IconName;
  /** 14 (inline/chips/stats, default) or 16 (headers/actions). */
  size?: IconSize;
  /**
   * Accessible label. INV-13: an icon never appears without a text label on its
   * first use per surface — either a visible sibling label OR this aria-label.
   * Pass `decorative` only when a visible sibling label already names the icon.
   */
  label?: string;
  /** Set when a visible text label sits beside the icon (then aria-hidden). */
  decorative?: boolean;
  className?: string;
}

/**
 * The one way a component draws an icon. Enforces stroke 1.5 + currentColor +
 * the sanctioned sizes, and the INV-13 labeling contract. Color is inherited
 * (`currentColor`) so the violet quarantine holds without any per-icon hue.
 */
export function Icon({ name, size = 14, label, decorative, className }: IconProps) {
  const Glyph = ICON[name];
  const a11y = decorative
    ? { 'aria-hidden': true as const }
    : { role: 'img' as const, 'aria-label': label ?? name };
  return (
    <Glyph
      width={size}
      height={size}
      strokeWidth={ICON_STROKE}
      className={className}
      data-icon={name}
      {...a11y}
    />
  );
}
