export const COLORS = {
  file: '#a78bfa',
  class: '#6366f1',
  function: '#059669',
  generic: '#64748b',
  ghost: '#3b3b5c',
  fire: '#ff6b35',
  teal: '#4ecdc4',
} as const;

/** Map node_type integer to a CSS color. */
export function nodeTypeColor(nodeType: number): string {
  switch (nodeType) {
    case 0: return COLORS.file;
    case 1: return COLORS.class;
    case 2: return COLORS.function;
    default: return COLORS.generic;
  }
}

/** Map node_type integer to a human-readable label. */
export function nodeTypeLabel(nodeType: number): string {
  switch (nodeType) {
    case 0: return 'file';
    case 1: return 'class';
    case 2: return 'fn';
    default: return 'node';
  }
}

/** Lerp two hex colors by t (0–1). */
export function lerpColor(a: string, b: string, t: number): string {
  const parse = (h: string) => [
    parseInt(h.slice(1, 3), 16),
    parseInt(h.slice(3, 5), 16),
    parseInt(h.slice(5, 7), 16),
  ];
  const [ar, ag, ab] = parse(a);
  const [br, bg, bb] = parse(b);
  const r = Math.round(ar + (br - ar) * t);
  const g = Math.round(ag + (bg - ag) * t);
  const bv = Math.round(ab + (bb - ab) * t);
  return `#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${bv.toString(16).padStart(2, '0')}`;
}

/** Activation-based color: cold (slate) → warm (fire). */
export function activationColor(score: number): string {
  const clamped = Math.max(0, Math.min(1, score));
  return lerpColor('#475569', COLORS.fire, clamped);
}
