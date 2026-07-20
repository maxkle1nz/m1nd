/*
 * Threshold logic — first-run as empty state, not wizard (HUMAN-LAYER-PRD §4A.2).
 *
 * The pure, DOM-free heart of onboarding: the localStorage keys that make it
 * "never returns" (INV-12) and the word-grained progress copy (INV-05 — words,
 * never a fabricated percent). Repository creation is intentionally absent while
 * no exact typed G2/G3 bootstrap consumer exists.
 */

// ── Persistence: onboarding never returns (INV-12) ────────────────────────────
// The Threshold renders only at zero brains; each orientation beat dismisses
// independently and persists. A returning user (≥1 brain OR a persisted dismissal)
// never meets it again. Keys are namespaced so they never collide.

export const LS_ORIENTATION_DISMISSED = 'm1nd.threshold.orientationDismissed'; // "1" once ESC'd forever
export const LS_BEAT_DISMISSED_PREFIX = 'm1nd.threshold.beat.'; // + beat id → "1"
export const LS_LAST_BRAIN = 'm1nd.hall.lastBrainId'; // last-visited brain (the tree-landing signal)

export type OrientationBeat = 'map' | 'anchors' | 'gaps';
export const ORIENTATION_BEATS: OrientationBeat[] = ['map', 'anchors', 'gaps'];

/** Minimal storage shape so this is testable without a browser. */
export interface KV {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

/** The whole orientation was dismissed forever (ESC). */
export function orientationDismissed(kv: KV): boolean {
  return kv.getItem(LS_ORIENTATION_DISMISSED) === '1';
}

/** Persist the "ESC — forever" (all beats gone, never again). */
export function dismissOrientationForever(kv: KV): void {
  kv.setItem(LS_ORIENTATION_DISMISSED, '1');
}

/** A single beat was dismissed. */
export function beatDismissed(kv: KV, beat: OrientationBeat): boolean {
  return orientationDismissed(kv) || kv.getItem(LS_BEAT_DISMISSED_PREFIX + beat) === '1';
}

export function dismissBeat(kv: KV, beat: OrientationBeat): void {
  kv.setItem(LS_BEAT_DISMISSED_PREFIX + beat, '1');
}

/** True when every beat is individually dismissed → the orientation is spent. */
export function allBeatsDismissed(kv: KV): boolean {
  return orientationDismissed(kv) || ORIENTATION_BEATS.every((b) => kv.getItem(LS_BEAT_DISMISSED_PREFIX + b) === '1');
}

/** Remember the last-visited brain — an expert lands in their work, not a menu. */
export function rememberLastBrain(kv: KV, instanceId: string): void {
  kv.setItem(LS_LAST_BRAIN, instanceId);
}

export function lastBrain(kv: KV): string | null {
  return kv.getItem(LS_LAST_BRAIN);
}

// ── Feature-detection: the one-call bootstrap vs the clobber-safe fallback ─────
// Public repository creation remains absent until an exact typed G2/G3 consumer
// exists; schema syntax never enables a mutation affordance.

// ── The clobber ban (INV-11, §4A.4) ───────────────────────────────────────────
// A foreign-path ingest is never offered by the current UI. Bound-brain re-read
// remains a separate, existing-brain action.

// ── Word-grained progress (INV-05 — never a fabricated percent) ───────────────
// The SSE `ingest` event is a COMPLETION event; there is no percent stream. The
// Threshold shows calm indeterminate progress with WORDS, then lands on the tree.

export type ThresholdPhase = 'idle' | 'reading' | 'done';

/** Calm progress copy — words, not a bar. The size hint is honest, not a %. */
export function progressCopy(phase: ThresholdPhase): string {
  switch (phase) {
    case 'reading':
      return 'Reading… a mid-size repo takes about a minute.';
    case 'done':
      return 'Done — opening your map.';
    default:
      return '';
  }
}

// ── The 3-beat orientation copy (§4A.2 table) ─────────────────────────────────
// Each beat = one sentence pinned to the region it describes, from the real north
// packet. No fake numbers — a beat with no data is simply not shown.

export interface OrientationCopy {
  beat: OrientationBeat;
  text: string;
}

/**
 * Build the orientation beats from real north-packet fields. Absent data yields
 * NO beat for that slot (never a fabricated one). `nodeCount`/`edgeCount` from the
 * binding fingerprint; `anchorLabels` from context.anchors; `hasGaps` gates the
 * violet gap card (rendered by the component, not here).
 */
export function orientationBeats(input: {
  nodeCount: number | null;
  edgeCount: number | null;
  anchorLabels: string[];
  memoryCount: number | null;
}): OrientationCopy[] {
  const beats: OrientationCopy[] = [];
  if (input.nodeCount != null && input.edgeCount != null) {
    beats.push({ beat: 'map', text: `Here's your map: ${input.nodeCount} files, ${input.edgeCount} connections.` });
  }
  if (input.anchorLabels.length > 0) {
    const top = input.anchorLabels.slice(0, 3).join(', ');
    beats.push({ beat: 'anchors', text: `These files carry the most weight: ${top}.` });
  }
  // The gaps beat is always present as the honest close — its body is the violet
  // gap card (component). When there are zero memories the copy says so honestly.
  const memoryLine =
    input.memoryCount != null && input.memoryCount === 0
      ? 'No memories yet — agents leave notes here as they work.'
      : "What I don't know yet";
  beats.push({ beat: 'gaps', text: memoryLine });
  return beats;
}
