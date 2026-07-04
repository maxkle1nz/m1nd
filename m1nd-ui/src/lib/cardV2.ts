/*
 * Card anatomy v2 — the precious fields (HUMAN-LAYER-PRD §4A.3.1).
 *
 * The Hall card answers three questions in calm typography: do I trust it? is it
 * alive? what has it learned? For the OPEN (bound) brain — the TODAY column — the
 * four GOLD fields earn the face; everything else is DEPTH (the receipt). A hosted
 * brain shows these fields ABSENT-honest (no fake), per the today-vs-2H table.
 *
 * This module is the pure, DOM-free heart: each function turns a REAL serialized
 * field into the one calm line the card renders. Anti-scope (binding): no
 * timeseries, no aggregate score, no animated percentages — a Hall card is a
 * specimen label, not a dashboard tile.
 */
import { STALE_AFTER_MS, humanizeAge } from './softProof';
import { isPostItNode, readCreatedMs, readSourceAgent, type GraphSnapshot } from './snapshot';
import type { AmIStaleOutput } from '../api/toolTypes';

// ── G1 — freshness vs. git (am_i_stale) ───────────────────────────────────────

export interface FreshnessG1 {
  /** How many inventoried files changed on disk since m1nd read them. */
  changed: number;
  /** The calm caption in action language. */
  caption: string;
  /** True when nothing changed — "everything I read is current". */
  allCurrent: boolean;
}

/** Partition an am_i_stale result into the G1 line. `null` → not-yet-checked. */
export function freshnessG1(result: AmIStaleOutput | null): FreshnessG1 | null {
  if (!result) return null;
  const changed = (result.stale ?? []).length;
  if (changed === 0) {
    return { changed: 0, caption: 'everything I read is current', allCurrent: true };
  }
  return {
    changed,
    caption: `${changed} ${changed === 1 ? 'file' : 'files'} changed since I read them`,
    allCurrent: false,
  };
}

// ── G2 — calibration chip (predict.calibration) ───────────────────────────────

export interface CalibrationBlock {
  calibrated: boolean;
  tau?: number;
  measured_precision?: number;
  coverage?: number;
  n?: number;
}

export interface CalibrationG2 {
  measured: boolean;
  /** The face caption (action language). */
  caption: string;
  /** The receipt-depth exact line (τ / precision / coverage / n), or null. */
  receipt: string | null;
}

/**
 * The G2 calibration chip. Uncalibrated → the engine's law verbatim: capped at
 * `reverify` (`act` UNREACHABLE). Calibrated → "measured here ✓" + the exact
 * row in the receipt.
 */
export function calibrationG2(cal: CalibrationBlock | null): CalibrationG2 | null {
  if (!cal) return null;
  if (!cal.calibrated) {
    return {
      measured: false,
      caption: "not measured on this repo yet — capped at reverify (act UNREACHABLE)",
      receipt: null,
    };
  }
  const pct = (x: number | undefined) => (x != null ? `${(x * 100).toFixed(0)}%` : '—');
  return {
    measured: true,
    caption: 'measured here ✓',
    receipt:
      `τ ${cal.tau?.toFixed(3) ?? '—'} · precision ${pct(cal.measured_precision)} · ` +
      `coverage ${pct(cal.coverage)} · n ${cal.n?.toLocaleString() ?? '—'}`,
  };
}

// ── G3 — the compounding meter (light:* nodes + 30d aging) ────────────────────

export interface CompoundingG3 {
  count: number;
  newestMs: number | null;
  aging: number;
  /** The one calm line: "14 memories · newest 2 h ago · 1 aging". */
  caption: string;
}

/**
 * Aggregate the compounding meter from the snapshot's L1GHT nodes (the proof the
 * brain gets richer, not just older). "aging" = the same 30-day rule `north`
 * applies, mirrored client-side. Counts DISTINCT memory docs (by slug), not every
 * fragment node.
 */
export function compoundingG3(snap: GraphSnapshot | null, now = Date.now()): CompoundingG3 | null {
  if (!snap) return null;
  // One memory = one doc slug; count the claim/section nodes, track newest + aging.
  const seenSlug = new Set<string>();
  let count = 0;
  let newestMs: number | null = null;
  let aging = 0;
  for (const n of snap.nodes) {
    if (!isPostItNode(n)) continue;
    const slugMatch = n.external_id.match(/^light::[^:]+::(?:file|section)::([^:]+)/);
    const slug = slugMatch ? slugMatch[1] : n.external_id;
    if (seenSlug.has(slug)) continue;
    seenSlug.add(slug);
    count += 1;
    const created = readCreatedMs(n);
    if (created != null) {
      if (newestMs == null || created > newestMs) newestMs = created;
      if (now - created >= STALE_AFTER_MS) aging += 1;
    }
  }
  const parts = [`${count} ${count === 1 ? 'memory' : 'memories'}`];
  if (newestMs != null) parts.push(`newest ${humanizeAge(Math.max(0, now - newestMs))}`);
  if (aging > 0) parts.push(`${aging} aging`);
  return { count, newestMs, aging, caption: parts.join(' · ') };
}

// ── G4 — aliveness (sessions + queries) ───────────────────────────────────────

export interface AlivenessG4 {
  sessions: number;
  queries: number;
  /** "2 agents attached · 48 queries this session" — one caption, not two rows. */
  caption: string;
}

/** The aliveness line for the open brain (self envelope). */
export function alivenessG4(input: { active_agent_sessions?: number; queries_processed?: number } | null): AlivenessG4 | null {
  if (!input) return null;
  const sessions = input.active_agent_sessions ?? 0;
  const queries = input.queries_processed ?? 0;
  return {
    sessions,
    queries,
    caption: `${sessions} ${sessions === 1 ? 'agent' : 'agents'} attached · ${queries} ${queries === 1 ? 'query' : 'queries'} this session`,
  };
}

// ── D1 — the last learned claim (receipt depth) ───────────────────────────────

export interface LastClaimD1 {
  claim: string;
  sourceAgent: string | null;
  ageMs: number | null;
}

/**
 * The newest L1GHT claim by `light:created` desc — one line in the receipt:
 * "latest claim label" · agent · age (absent → "author unknown", INV-04).
 */
export function lastClaimD1(snap: GraphSnapshot | null, now = Date.now()): LastClaimD1 | null {
  if (!snap) return null;
  let best: { claim: string; sourceAgent: string | null; created: number | null } | null = null;
  const seenSlug = new Set<string>();
  for (const n of snap.nodes) {
    if (!isPostItNode(n)) continue;
    const slugMatch = n.external_id.match(/^light::[^:]+::(?:file|section)::([^:]+)/);
    const slug = slugMatch ? slugMatch[1] : n.external_id;
    if (seenSlug.has(slug)) continue;
    seenSlug.add(slug);
    const created = readCreatedMs(n);
    if (best == null || (created != null && (best.created == null || created > best.created))) {
      best = { claim: n.label, sourceAgent: readSourceAgent(n), created };
    }
  }
  if (!best) return null;
  return {
    claim: best.claim,
    sourceAgent: best.sourceAgent,
    ageMs: best.created != null ? Math.max(0, now - best.created) : null,
  };
}

// ── D2 — honest gaps (receipt depth) ──────────────────────────────────────────

export interface HonestGapsD2 {
  visited: number;
  total: number;
  ghostEdges: number | null;
  gaps: string[];
  /** "51 of 6,520 files visited" line. */
  coverageLine: string;
}

/** The honest-gaps receipt block: coverage + ghost edges + north honest_gaps. */
export function honestGapsD2(input: {
  coverage?: { visited?: number; total?: number } | null;
  ghostEdges?: number | null;
  gaps?: string[] | null;
}): HonestGapsD2 | null {
  const visited = input.coverage?.visited ?? null;
  const total = input.coverage?.total ?? null;
  if (visited == null && total == null && !input.gaps?.length) return null;
  return {
    visited: visited ?? 0,
    total: total ?? 0,
    ghostEdges: input.ghostEdges ?? null,
    gaps: input.gaps ?? [],
    coverageLine:
      total != null
        ? `${(visited ?? 0).toLocaleString()} of ${total.toLocaleString()} files visited`
        : `${(visited ?? 0).toLocaleString()} visited`,
  };
}
