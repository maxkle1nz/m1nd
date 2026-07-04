/*
 * Reading-the-Tree lenses + filters (HUMAN-LAYER-PRD §4A.10).
 *
 * The tree's grammar (§3) is right; its READING instruments were missing. This
 * module is the pure, DOM-free heart of the three added instruments — GROUPING
 * (directory | kind | layer), FILTERING (six matte chips, each a real field),
 * and the name-search predicate — every one drawing fields already serialized,
 * most of it client-side over the snapshot the tree already fetched.
 *
 * Groups behave EXACTLY like directories (carets, counts, keyboard) so there is
 * one navigation grammar, not two. Filter honesty is structural: a filtered view
 * always knows how many rows it hid ("N hidden by filters").
 */
import { NODE_TYPE } from './snapshot';
import type { TreeRow } from './tree';
import type { LayersOutput } from '../api/toolTypes';
import type { TrustBand } from './softProof';

// ── The three lenses ──────────────────────────────────────────────────────────

export type Lens = 'directory' | 'kind' | 'layer';

/** A group header row (behaves like a directory: caret, count, children). */
export interface GroupRow {
  id: string;
  /** Display name of the group ("functions", "entry_points · L1", "unlayered"). */
  label: string;
  /** How many leaf rows fall under it (tabular, right-aligned — INV-13). */
  count: number;
  /** The leaf rows (file/symbol) in this group, already flattened. */
  rows: TreeRow[];
}

/** The KIND buckets, in the §4A.10 order: file · function · struct/class · doc. */
const KIND_ORDER: Array<{ key: string; label: string; matches: (t: number | undefined) => boolean }> = [
  { key: 'file', label: 'files', matches: (t) => t === NODE_TYPE.File },
  { key: 'function', label: 'functions', matches: (t) => t === NODE_TYPE.Function },
  {
    key: 'struct',
    label: 'structs & classes',
    matches: (t) => t === NODE_TYPE.Struct || t === NODE_TYPE.Class || t === NODE_TYPE.Enum,
  },
  {
    key: 'doc',
    label: 'types & modules',
    matches: (t) => t === NODE_TYPE.Type || t === NODE_TYPE.Module,
  },
];

/** Collect every file/symbol leaf row from an assembled tree (depth-first). */
export function collectLeafRows(root: TreeRow): TreeRow[] {
  const out: TreeRow[] = [];
  const walk = (r: TreeRow) => {
    for (const c of r.children) {
      if (c.kind === 'file' || c.kind === 'symbol') out.push(c);
      walk(c);
    }
  };
  walk(root);
  return out;
}

/** Group leaf rows by KIND (client-only regroup over snapshot node_type). */
export function groupByKind(root: TreeRow): GroupRow[] {
  const leaves = collectLeafRows(root);
  const groups: GroupRow[] = [];
  for (const bucket of KIND_ORDER) {
    const rows = leaves.filter((r) => bucket.matches(r.nodeType)).sort((a, b) => a.path.localeCompare(b.path));
    if (rows.length > 0) {
      groups.push({ id: `kind:${bucket.key}`, label: bucket.label, count: rows.length, rows });
    }
  }
  // Anything not matched by a named bucket lands in an honest "other" group —
  // never hidden (the same law as the layer lens's "unlayered").
  const claimed = new Set(groups.flatMap((g) => g.rows.map((r) => r.id)));
  const other = leaves.filter((r) => !claimed.has(r.id)).sort((a, b) => a.path.localeCompare(b.path));
  if (other.length > 0) {
    groups.push({ id: 'kind:other', label: 'other', count: other.length, rows: other });
  }
  return groups;
}

/**
 * Group leaf rows by architectural LAYER using the `layers` verb output. Each
 * detected layer becomes one group ("name · L<level>", "N nodes" verbatim); every
 * leaf row whose node is in NO layer lands in an honest **"unlayered"** group —
 * never hidden (§4A.10). Layer names repeat across levels, so the group key is
 * the level.
 */
export function groupByLayer(root: TreeRow, layers: LayersOutput): GroupRow[] {
  const leaves = collectLeafRows(root);
  const rowByExternalId = new Map<string, TreeRow>();
  for (const r of leaves) if (r.externalId) rowByExternalId.set(r.externalId, r);

  const groups: GroupRow[] = [];
  const assigned = new Set<string>();
  // Sort layers by level for a stable reading order.
  const sorted = [...layers.layers].sort((a, b) => a.level - b.level);
  for (const layer of sorted) {
    const rows: TreeRow[] = [];
    for (const n of layer.nodes) {
      const row = rowByExternalId.get(n.node_id);
      if (row && !assigned.has(row.id)) {
        rows.push(row);
        assigned.add(row.id);
      }
    }
    // Keep the group even if its (truncated) node sample mapped no visible rows —
    // its real count is the engine's `node_count`, shown verbatim.
    groups.push({
      id: `layer:${layer.level}`,
      label: `${layer.name} · L${layer.level}`,
      count: layer.node_count,
      rows: rows.sort((a, b) => a.path.localeCompare(b.path)),
    });
  }

  // The honest "unlayered" group — every leaf row not claimed by any layer.
  const unlayered = leaves
    .filter((r) => !assigned.has(r.id))
    .sort((a, b) => a.path.localeCompare(b.path));
  if (unlayered.length > 0) {
    groups.push({ id: 'layer:unlayered', label: 'unlayered', count: unlayered.length, rows: unlayered });
  }
  return groups;
}

// ── The six filter chips (§4A.10) — each a real field, AND-combined ───────────

export type FilterKey = 'kind' | 'language' | 'trust' | 'hasMemory' | 'changed' | 'churning';

/** The context a filter needs, all already fetched or one cheap call. */
export interface FilterContext {
  /** external_id → trust band (from the `trust` verb the dots already fetch). */
  bands: Map<string, TrustBand>;
  /** source_paths currently churning (from `tremor`, already fetched). */
  breathingPaths: Set<string>;
  /** file paths flagged changed-since-read (from `am_i_stale`, on demand). */
  stalePaths: Set<string>;
  /** the kinds selected in the `kind` chip (node_type set); empty → chip off. */
  kinds: Set<number>;
  /** the language extensions selected (e.g. "rs","ts"); empty → chip off. */
  languages: Set<string>;
  /** the trust bands selected; empty → chip off. */
  trustBands: Set<TrustBand>;
  /** which chips are active (a chip with no selection but toggled still filters). */
  active: Set<FilterKey>;
}

/** DERIVED language from a file path extension (a UI constant — the engine
 *  asserts nothing about language; §4A.10 states it as derived). */
export function languageOf(path: string): string | null {
  const m = path.match(/\.([a-z0-9]+)$/i);
  return m ? m[1].toLowerCase() : null;
}

/**
 * Does a leaf row survive the active filters? Chips AND-combine; a chip that is
 * not active is a no-op. The predicate is pure over the row + context.
 */
export function rowPasses(row: TreeRow, ctx: FilterContext, bandFor: (id?: string) => TrustBand): boolean {
  if (ctx.active.has('kind')) {
    if (row.nodeType == null || !ctx.kinds.has(row.nodeType)) return false;
  }
  if (ctx.active.has('language')) {
    const lang = languageOf(row.path);
    if (!lang || !ctx.languages.has(lang)) return false;
  }
  if (ctx.active.has('trust')) {
    if (!ctx.trustBands.has(bandFor(row.externalId))) return false;
  }
  if (ctx.active.has('hasMemory')) {
    if (row.postIts.length === 0) return false;
  }
  if (ctx.active.has('changed')) {
    // Only file rows can be stale; the chip keeps rows whose path is flagged.
    if (!ctx.stalePaths.has(row.path)) return false;
  }
  if (ctx.active.has('churning')) {
    if (!ctx.breathingPaths.has(row.path)) return false;
  }
  return true;
}

/** The name-search predicate — the shipped instant substring over name/path. */
export function nameMatches(row: TreeRow, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return row.name.toLowerCase().includes(q) || row.path.toLowerCase().includes(q);
}

/**
 * The residue line (§4A.10 filter honesty): "N rows · M hidden by filters".
 * `total` is the unfiltered leaf count, `shown` the surviving count.
 */
export function filterResidue(shown: number, total: number): { shown: number; hidden: number } {
  return { shown, hidden: Math.max(0, total - shown) };
}

// ── The meaning-search honesty caption (§4A.10) ───────────────────────────────

/**
 * The trigram-fallback caption. When `embeddings_used` is false the match was
 * lexical, not semantic — the fallback is WORN, not hidden. When true, no caption
 * (the match really was by meaning). A pure function so both branches are testable
 * against the real envelope's boolean without a hand-written fixture.
 */
export function textNotMeaningCaption(embeddingsUsed: boolean | undefined): string | null {
  return embeddingsUsed === false ? 'matched by text, not meaning' : null;
}

/**
 * INV-16: a meaning result belongs to the viewed brain iff its file_path is one
 * the viewed brain's snapshot knows. `knownPaths` is the set of source_paths in
 * the served snapshot; a hit whose file_path is outside it (a stale panel across
 * an Open switch, a wrong-echo response) is DROPPED, never rendered into the
 * wrong tree. Memory/light hits (no file_path) are kept — they are the viewed
 * brain's own memory namespace by construction of the served envelope.
 */
export function resultBelongsToBrain(filePath: string | undefined, knownPaths: Set<string>): boolean {
  if (!filePath) return true; // memory/light node — from the served brain's own namespace
  return knownPaths.has(filePath);
}
