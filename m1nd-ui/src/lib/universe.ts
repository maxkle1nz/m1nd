/*
 * Universe semantics — the pure functions behind the L0 panorama
 * (HUMAN-VIEW-V2 F30). No React, no fetch: the wire types, the world SCALE
 * (size ∝ node_count on a log curve, light ∝ freshness), the deterministic
 * phyllotaxis LAYOUT, the client-composed serif HEADLINE (universe FACTS —
 * counts, never a cross-brain vital), and the LANDING queue (reads aggregated;
 * every write still travels its own per-type verb). Each is unit-tested;
 * UniverseView + TheLanding are thin renders over these.
 */
import { compactAge } from './presence';

/** One world's live presence (the P1 wire entry, this world only). */
export interface UniversePresence {
  agent_id: string;
  root: string;
  caller_root?: string;
  first_seen_ms: number;
  last_seen_ms: number;
  query_count: number;
  mutation?: { observed_at_ms?: number | null; declared_intent?: string | null };
  task_ref?: string;
}

/** One world — an EXISTING project brain, read sidecar-only (F30 §3). Size +
 *  freshness are OMITTED (not zero) when the manifest never recorded them. */
export interface UniverseWorld {
  key: string;
  root: string;
  name: string;
  node_count?: number;
  edge_count?: number;
  updated_ms?: number;
  awake: boolean;
  presences: UniversePresence[];
  pending: { stamps: number; ratifies: number };
  letters: { merge_wait: number; total: number };
}

/** The `GET /api/universe` body (`m1nd-universe-v0`). */
export interface UniverseResponse {
  schema: string;
  worlds: UniverseWorld[];
  owner: { alerts_pending: number };
  totals: { worlds: number; awake: number; pending: number };
}

/** Total pending human gestures on one world (stamps + ratifies). Alerts are
 *  owner-scope, never a world's — they never fold in here. */
export function worldPending(w: UniverseWorld): number {
  return w.pending.stamps + w.pending.ratifies;
}

// ── SCALE ─────────────────────────────────────────────────────────────────

export const WORLD_R_MIN = 15;
export const WORLD_R_MAX = 46;
/** Node count at which a world reaches WORLD_R_MAX — a soft reference, not a cap
 *  on the data (a bigger repo simply pins at the max radius). */
const WORLD_N_REF = 40_000;

/**
 * A world's radius — size ∝ node_count on a LOG curve so a 200-node repo and a
 * 40k-node repo both read at a glance without the small one vanishing. A world
 * with no recorded count (a pre-counts manifest) renders at the floor — honest
 * "size unknown", never a fabricated large disc.
 */
export function worldRadius(nodeCount: number | undefined): number {
  if (nodeCount == null || nodeCount <= 0) return WORLD_R_MIN;
  const t = Math.min(1, Math.log(nodeCount + 1) / Math.log(WORLD_N_REF + 1));
  return WORLD_R_MIN + (WORLD_R_MAX - WORLD_R_MIN) * t;
}

/** Presentational light floor — a stale world dims but never goes dark (its age
 *  is shown as DATA beside it; opacity is only the cue). */
export const WORLD_LIGHT_FLOOR = 0.4;
/** Freshness horizon: a world older than this sits at the light floor. */
const WORLD_STALE_HORIZON_MS = 14 * 24 * 60 * 60 * 1000; // 14 days

/**
 * A world's light — opacity ∝ freshness of `updated_ms`, linear from full at
 * "just now" down to WORLD_LIGHT_FLOOR at the stale horizon. An ABSENT
 * `updated_ms` returns a neutral mid (the caller shows "age unknown" beside it):
 * the manifest may lag the live store, and the UI says so — stale light is DATA.
 */
export function worldLight(updatedMs: number | undefined, nowMs = Date.now()): number {
  if (updatedMs == null) return 0.55;
  const age = Math.max(0, nowMs - updatedMs);
  const t = Math.min(1, age / WORLD_STALE_HORIZON_MS);
  return WORLD_LIGHT_FLOOR + (1 - WORLD_LIGHT_FLOOR) * (1 - t);
}

/** The honest age label a world wears ("now"/"42s"/"3h"/"2d", or "age unknown"
 *  when the manifest recorded no freshness). Reuses the presence formatter. */
export function worldAgeLabel(updatedMs: number | undefined, nowMs = Date.now()): string {
  if (updatedMs == null) return 'age unknown';
  return compactAge(updatedMs, nowMs);
}

// ── LAYOUT ───────────────────────────────────────────────────────────────

export interface WorldPlacement {
  world: UniverseWorld;
  cx: number;
  cy: number;
  r: number;
  light: number;
  pending: number;
}

/** The golden angle — the phyllotaxis (sunflower) constant. A calm, organic,
 *  DETERMINISTIC scatter: no physics sim, identical every render, testable. */
const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));

/**
 * Place worlds around the observatory origin on a phyllotaxis spiral — index 0
 * nearest the centre, each next one golden-angle-rotated and sqrt-further out.
 * Deterministic (order in = order out); `spacing` scales the whole field to the
 * viewport. Radius + light + pending are precomputed so the render stays dumb.
 */
export function layoutWorlds(
  worlds: UniverseWorld[],
  opts: { width: number; height: number; nowMs?: number },
): WorldPlacement[] {
  const { width, height } = opts;
  const nowMs = opts.nowMs ?? Date.now();
  const cx0 = width / 2;
  const cy0 = height / 2;
  // Scale the spiral so the outermost world stays inside the frame with margin.
  const span = Math.min(width, height) / 2 - WORLD_R_MAX - 24;
  const spacing = worlds.length <= 1 ? 0 : span / Math.sqrt(worlds.length - 1);
  return worlds.map((world, i) => {
    const angle = i * GOLDEN_ANGLE;
    const dist = spacing * Math.sqrt(i);
    return {
      world,
      cx: cx0 + dist * Math.cos(angle),
      cy: cy0 + dist * Math.sin(angle),
      r: worldRadius(world.node_count),
      light: worldLight(world.updated_ms, nowMs),
      pending: worldPending(world),
    };
  });
}

// ── HEADLINE (universe FACTS — counts, never a cross-brain vital) ───────────

function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}

/**
 * The L0 serif sentence — client-composed from universe FACTS only (world count,
 * awake count, pending total). NEVER a cross-brain pulse or summed vital (the
 * pulse stays PER-BRAIN by ratified law). Reads calm and honest at every count.
 */
export function universeHeadline(totals: { worlds: number; awake: number; pending: number }): string {
  const worlds = plural(totals.worlds, 'world');
  const awake = totals.awake > 0 ? `${totals.awake} awake` : 'none awake';
  const pending =
    totals.pending > 0 ? `${totals.pending} await your hand` : 'nothing awaits your hand';
  return `${worlds} · ${awake} · ${pending}`;
}

// ── THE LANDING (the unified gesture queue) ────────────────────────────────

export type LandingKind = 'stamp' | 'ratify' | 'alert';

export interface LandingItem {
  /** Stable list key. */
  id: string;
  kind: LandingKind;
  /** The chip: a world's name, or the literal `owner` for a daemon alert (alerts
   *  live on the owner, never a brain). */
  chip: string;
  /** 'world' items navigate to that world's room; 'owner' to the Hall. */
  scope: 'world' | 'owner';
  /** The world root to open (absent for owner-scope items). */
  worldRoot?: string;
  count: number;
  /** One-line human explanation (sentence case, never "bell"). */
  line: string;
}

/**
 * Flatten the panorama into ONE queue, every world plus the owner: a per-world
 * bucket for stamps and for ratifies (only when > 0), then the owner's alert
 * bucket. Reads aggregated; clicking an item navigates to the existing per-type
 * flow — this composes NO write. Order: worlds in panorama order (stamps before
 * ratifies within a world), owner alerts last.
 */
export function buildLandingItems(universe: UniverseResponse): LandingItem[] {
  const items: LandingItem[] = [];
  for (const w of universe.worlds) {
    if (w.pending.stamps > 0) {
      items.push({
        id: `${w.key}:stamp`,
        kind: 'stamp',
        chip: w.name,
        scope: 'world',
        worldRoot: w.root,
        count: w.pending.stamps,
        line: `${plural(w.pending.stamps, 'receipt')} await your stamp`,
      });
    }
    if (w.pending.ratifies > 0) {
      items.push({
        id: `${w.key}:ratify`,
        kind: 'ratify',
        chip: w.name,
        scope: 'world',
        worldRoot: w.root,
        count: w.pending.ratifies,
        line: `${plural(w.pending.ratifies, 'block')} await ratification`,
      });
    }
  }
  if (universe.owner.alerts_pending > 0) {
    items.push({
      id: 'owner:alerts',
      kind: 'alert',
      chip: 'owner',
      scope: 'owner',
      count: universe.owner.alerts_pending,
      line: `${plural(universe.owner.alerts_pending, 'daemon alert')} to acknowledge`,
    });
  }
  return items;
}
