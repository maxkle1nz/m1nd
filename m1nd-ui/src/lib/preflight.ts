/*
 * preflight — the north packet → Pre-Flight Card view model (HUMAN-LAYER §4.2,
 * ORGANISM §C1 reader-2). Pure functions only: the card is a RENDERING, so every
 * derived value here traces to a real packet field. A field with no packet field
 * behind it is fabrication (INV-01) — these helpers return absent, never invent.
 *
 * The §C1 law made concrete: the SAME north packet an agent receives, read for a
 * human. This module decides only the READING (action language, floor language,
 * absent-honest), never new data.
 */
import type { NorthPacket, NorthMemoryEntry } from '../api/toolTypes';
import { tierToBand, type TrustBand } from './softProof';

// ── BINDING beat — which brain, trust mode, freshness (§4.2 header) ────────────

export interface BindingView {
  /** The graph's own node/edge counts — absent when the fingerprint omits them. */
  nodeCount: number | null;
  edgeCount: number | null;
  binaryVersion: string | null;
  /** The engine's raw trust_mode (rung 2 word); the human line is `trustLine`. */
  trustMode: string;
  /** Action-language rendering of the binding trust (rung 0–1, no epistemology). */
  trustLine: string;
  /** True only when the binding is NOT full trust (needs_ingest / degraded). */
  degraded: boolean;
}

interface Fingerprint {
  node_count?: number;
  edge_count?: number;
  binary_version?: string;
}

/**
 * The binding beat. `trust_mode === "full_trust"` reads as a calm "grounded here";
 * anything else reads as "orientation only, verify locally" — the §2 anxiety
 * principle (uncertainty → next-action guidance, never raw epistemology).
 */
export function bindingView(packet: NorthPacket): BindingView {
  const fp = (packet.binding as { fingerprint?: Fingerprint } | undefined)?.fingerprint;
  const mode = packet.binding.trust_mode;
  const full = mode === 'full_trust';
  return {
    nodeCount: fp?.node_count ?? null,
    edgeCount: fp?.edge_count ?? null,
    binaryVersion: fp?.binary_version ?? null,
    trustMode: mode,
    trustLine: full
      ? 'grounded in this repo'
      : "I'm not fully bound here — I'll verify against your files as I go",
    degraded: !full,
  };
}

// ── The reception rider (§C1.4 / JOINT-I) — same verdict, zero new data ────────
// The reception match echoes on the Card's binding header. A `caller_root_mismatch`
// means the bound graph does NOT cover the caller's repo — surfaced honestly, in
// action language, never hidden behind a confident header.

export interface ReceptionView {
  /** True when the bound graph does not cover the caller's current repo. */
  mismatch: boolean;
  /** The engine's own honest one-liner, rendered verbatim (never reworded). */
  honest: string | null;
}

interface ReceptionBlock {
  match?: string;
  honest?: string;
}

export function receptionView(packet: NorthPacket): ReceptionView {
  const r = (packet as { reception?: ReceptionBlock }).reception;
  if (!r) return { mismatch: false, honest: null };
  return {
    mismatch: r.match === 'caller_root_mismatch',
    honest: r.honest ?? null,
  };
}

// ── ANCHORS beat — the task's focus regions (§4.2 mini-map strip) ──────────────
// The neighborhood: focus_nodes + the top PageRank anchors, as a small strip
// (NOT a graph). Labels only — the card never renders a node m1nd didn't return.

export interface AnchorChip {
  label: string;
  nodeId: string;
}

interface RawAnchor {
  label?: string;
  node_id?: string;
}
interface RawFocus {
  label?: string;
  node_id?: string;
  path?: string;
}

/** The focus regions the task activated — focus_nodes first, then anchors, deduped
 *  by node_id, capped so the strip stays a strip (§3.4 hairball line). Absent
 *  context → empty (the beat simply does not render). */
export function anchorChips(packet: NorthPacket, cap = 6): AnchorChip[] {
  const ctx = packet.context ?? {};
  const focus = ((ctx.focus_nodes ?? []) as RawFocus[])
    .map((f) => ({ label: f.label ?? '', nodeId: f.node_id ?? '' }))
    .filter((c) => c.label && c.nodeId);
  const anchors = ((ctx.anchors ?? []) as RawAnchor[])
    .map((a) => ({ label: a.label ?? '', nodeId: a.node_id ?? '' }))
    .filter((c) => c.label && c.nodeId);
  const seen = new Set<string>();
  const out: AnchorChip[] = [];
  for (const c of [...focus, ...anchors]) {
    if (seen.has(c.nodeId)) continue;
    seen.add(c.nodeId);
    out.push(c);
    if (out.length >= cap) break;
  }
  return out;
}

// ── WHAT AGENTS KNOW beat — the memory strip (§4.2, R7 tier+origin rows) ────────
// Each claim carries author + age + (from R7) tier + origin_brain. Absent age or
// author renders "unknown", never faked (INV-04). This is the compounding proof:
// what prior agents PROVED here.

export interface MemoryRow {
  claim: string;
  sourceAgent: string | null;
  ageMs: number | null;
  stale: boolean;
  /** R7: the storage tier (`medulla` / `project`), absent on pre-R7 rows. */
  tier: string | null;
  /** R7: the brain that authored it, absent when not cross-brain-labeled. */
  originBrain: string | null;
  nodeId: string;
}

interface MemoryEntryR7 extends NorthMemoryEntry {
  tier?: string;
  origin_brain?: string;
}

export function memoryRows(packet: NorthPacket): MemoryRow[] {
  return (packet.memory ?? []).map((m) => {
    const r7 = m as MemoryEntryR7;
    return {
      claim: m.claim,
      sourceAgent: m.source_agent ?? null,
      ageMs: m.age_ms ?? null,
      stale: m.stale ?? false,
      tier: r7.tier ?? null,
      originBrain: r7.origin_brain ?? null,
      nodeId: m.node_id,
    };
  });
}

/**
 * The honest "no task-relevant memory" line — R0's `memory_exists` makes it
 * truthful. When the store is NON-EMPTY but recall surfaced nothing for this task,
 * we say exactly that (recall miss, not empty store); a truly empty store says so.
 * Absent `memory_exists` (pre-R0 packet) → the generic honest line.
 */
export function emptyMemoryLine(packet: NorthPacket): string {
  const n = (packet as { memory_exists?: number }).memory_exists;
  if (typeof n === 'number' && n > 0) {
    return `No notes surfaced for this task — ${n} exist in the brain, none matched yet.`;
  }
  if (typeof n === 'number' && n === 0) {
    return 'No notes here yet — agents leave notes as they work.';
  }
  return 'No notes surfaced for this task yet.';
}

// ── HONEST GAPS beat — what m1nd does NOT know (§4.2 violet card, first-class) ──
// Rendered through the GapCard (action language, §2). This module only exposes the
// raw gap strings; GapCard owns the action-language mapping.

export function honestGaps(packet: NorthPacket): string[] {
  return packet.honest_gaps ?? [];
}

// ── VERDICT surface — act / reverify / abstain (§4.2, the calm decision) ───────
// The card's verdict is the READING of the packet's own sufficiency + binding, in
// the trust-ladder grammar (grammar 1). It never invents a confidence number.
//   - degraded binding OR needs_ingest → abstain (violet): "I won't guess this one"
//   - sufficiency gathering/insufficient → reverify: "worth a second look"
//   - grounded + sufficient → act: "good to go"

export type PreflightVerdict = 'act' | 'reverify' | 'abstain';

export function preflightVerdict(packet: NorthPacket): PreflightVerdict {
  if (packet.needs === 'needs_ingest') return 'abstain';
  if (packet.binding.trust_mode !== 'full_trust') return 'abstain';
  const state = packet.sufficiency?.state;
  if (state === 'sufficient' || state === 'saturated') return 'act';
  // gathering, or absent sufficiency → not enough to say "go" yet.
  return 'reverify';
}

// ── The primary next-move button (§4.2 headline action) ────────────────────────

/** The card's one primary action, from `next_move` — surfaced as the button.
 *  Absent → null (no fabricated action; the headline stands alone). */
export function nextMove(packet: NorthPacket): string | null {
  return packet.next_move ?? null;
}

// ── The trust dot band for the seeded focus node (header glance) ────────────────
// The card is seeded with a focus node; its band is the FIRST focus node's band
// when the tree passed one. When no band is known, insufficient_evidence (honest).

export function seedBand(band: TrustBand | null | undefined): TrustBand {
  return band ?? 'insufficient_evidence';
}

/** Re-export so the card imports the tier→band map from one place. */
export { tierToBand };
