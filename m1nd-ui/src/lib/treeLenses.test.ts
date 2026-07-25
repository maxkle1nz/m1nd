/*
 * Reading-the-Tree lenses/filters/search — HUMAN-LAYER-PRD §4A.10.
 * Fed by REAL captured envelopes: snapshot.compact.json (the tree),
 * layers.json (the `layers` verb), seek_meaning.json (a real SeekOutput).
 * INV-16: search/filters never leave the viewed brain.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { buildTree } from './tree';
import type { GraphSnapshot } from './snapshot';
import { NODE_TYPE } from './snapshot';
import type { LayersOutput, SeekOutput } from '../api/toolTypes';
import type { TrustBand } from './softProof';
import {
  collectLeafRows,
  groupByKind,
  groupByLayer,
  rowPasses,
  languageOf,
  nameMatches,
  filterResidue,
  residueHiddenBy,
  textNotMeaningCaption,
  resultBelongsToBrain,
  type FilterContext,
} from './treeLenses';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');
const load = <T,>(name: string): T => JSON.parse(readFileSync(join(FIX, name), 'utf8'));

const snap = load<GraphSnapshot>('snapshot.compact.json');
const layers = load<LayersOutput>('layers.json');
const seek = load<SeekOutput>('seek_meaning.json');
const root = buildTree(snap);

// ── Lenses ────────────────────────────────────────────────────────────────────
test('§4A.10: the kind lens groups leaves by node_type with real counts, nothing hidden', () => {
  const leaves = collectLeafRows(root);
  const groups = groupByKind(root);
  assert.ok(groups.length > 0, 'at least one kind group');
  // Every leaf lands in exactly one group (nothing hidden).
  const grouped = groups.reduce((n, g) => n + g.rows.length, 0);
  assert.equal(grouped, leaves.length, 'every leaf row is grouped — nothing silently dropped');
  // The files group count matches the real file-row count.
  const fileRows = leaves.filter((r) => r.nodeType === NODE_TYPE.File).length;
  const filesGroup = groups.find((g) => g.id === 'kind:file');
  if (fileRows > 0) assert.equal(filesGroup?.count, fileRows, 'files group count is exact');
});

test('§4A.10: the layer lens renders real `layers` output + the honest "unlayered" group', () => {
  const groups = groupByLayer(root, layers);
  // One group per detected layer, in level order, plus (usually) unlayered.
  const layerGroups = groups.filter((g) => g.id.startsWith('layer:') && g.id !== 'layer:unlayered');
  assert.equal(layerGroups.length, layers.layers.length, 'one group per detected layer');
  // Group counts are the engine's node_count VERBATIM (not the mapped-sample size).
  for (const lg of layerGroups) {
    const level = Number(lg.id.split(':')[1]);
    const src = layers.layers.find((l) => l.level === level)!;
    assert.equal(lg.count, src.node_count, `layer L${level} count is the engine's node_count verbatim`);
    assert.match(lg.label, new RegExp(`· L${level}$`), 'the label disambiguates repeated names by level');
  }
  // The honest unlayered group exists (the compact snapshot has rows outside the
  // truncated layer samples) and is never hidden.
  const unlayered = groups.find((g) => g.id === 'layer:unlayered');
  assert.ok(unlayered, 'an "unlayered" group is present — rows outside every layer are never hidden');
  assert.equal(unlayered!.label, 'unlayered');
});

// ── Filters ─────────────────────────────────────────────────────────────────
function ctx(over: Partial<FilterContext>): FilterContext {
  return {
    bands: new Map(),
    breathingPaths: new Set(),
    stalePaths: new Set(),
    kinds: new Set(),
    languages: new Set(),
    trustBands: new Set(),
    active: new Set(),
    ...over,
  };
}
const bandFor = (bands: Map<string, TrustBand>) => (id?: string) =>
  (id && bands.get(id)) || ('insufficient_evidence' as TrustBand);

test('§4A.10: languageOf derives an extension → language (DERIVED, engine asserts nothing)', () => {
  assert.equal(languageOf('m1nd-mcp/src/http_server.rs'), 'rs');
  assert.equal(languageOf('m1nd-ui/src/App.tsx'), 'tsx');
  assert.equal(languageOf('README'), null, 'no extension → null (honest)');
});

test('§4A.10: the kind filter keeps only the selected node_types (AND-combine, chip off = no-op)', () => {
  const leaves = collectLeafRows(root);
  const c = ctx({ active: new Set(['kind']), kinds: new Set([NODE_TYPE.File]) });
  const kept = leaves.filter((r) => rowPasses(r, c, bandFor(c.bands)));
  assert.ok(kept.every((r) => r.nodeType === NODE_TYPE.File), 'only File rows survive the kind filter');
  // Chip off → no-op (everything passes).
  const off = ctx({});
  assert.equal(leaves.filter((r) => rowPasses(r, off, bandFor(off.bands))).length, leaves.length);
});

test('§4A.10: the "has memory" filter keeps only rows with ≥1 anchored post-it', () => {
  const leaves = collectLeafRows(root);
  const c = ctx({ active: new Set(['hasMemory']) });
  const kept = leaves.filter((r) => rowPasses(r, c, bandFor(c.bands)));
  assert.ok(kept.every((r) => r.postIts.length > 0), 'only rows with memories survive');
});

test('§4A.10: the "changed since read" filter keeps only am_i_stale-flagged paths', () => {
  const leaves = collectLeafRows(root);
  const somePath = leaves.find((r) => r.kind === 'file')?.path;
  assert.ok(somePath, 'a file row exists');
  const c = ctx({ active: new Set(['changed']), stalePaths: new Set([somePath!]) });
  const kept = leaves.filter((r) => rowPasses(r, c, bandFor(c.bands)));
  assert.ok(kept.every((r) => r.path === somePath), 'only the flagged path survives the changed filter');
});

test('§4A.10: filter residue is exact — "N rows · M hidden" never lies', () => {
  const r = filterResidue(41, 6520);
  assert.deepEqual(r, { shown: 41, hidden: 6479 });
  // clearing (shown === total) → zero hidden.
  assert.deepEqual(filterResidue(6520, 6520), { shown: 6520, hidden: 0 });
});

test('§4A.10: residue attribution blames the lens, not a filter the human never set', () => {
  // Zero user filters/search → the hidden rows are the LENS's subset, never "filters".
  assert.equal(residueHiddenBy(false), 'lens', 'a flat lens hiding rows is not a filter');
  // An active filter/search → the caption is honestly "filters".
  assert.equal(residueHiddenBy(true), 'filters');
});

// ── Search: name mode ─────────────────────────────────────────────────────────
test('§4A.10: name search is the instant substring over name + path', () => {
  const leaves = collectLeafRows(root);
  const hit = leaves.find((r) => r.name.includes('.'));
  assert.ok(hit, 'a named leaf exists');
  assert.ok(nameMatches(hit!, hit!.name.slice(0, 3)), 'matches a name substring');
  assert.ok(nameMatches(hit!, ''), 'empty query matches everything');
});

// ── Search: meaning mode — the panel's precious fields + INV-16 ───────────────
test('§4A.10: the real SeekOutput carries sufficiency (state + why) and a verdict — the panel renders them', () => {
  assert.ok(seek.sufficiency, 'sufficiency is always present on SeekOutput');
  assert.ok(typeof seek.sufficiency.state === 'string' && seek.sufficiency.state.length > 0, 'state is a plain word');
  assert.ok(typeof seek.sufficiency.why === 'string' && seek.sufficiency.why.length > 0, 'why is the verbatim engine line');
  assert.ok(['act', 'reverify', 'abstain'].includes(seek.trust_envelope.verdict), 'a real verdict');
  // The uncalibrated cap (G2 law): calibrated:false → the verdict is capped at reverify.
  if (seek.trust_envelope.calibrated === false) {
    assert.notEqual(seek.trust_envelope.verdict, 'act', 'uncalibrated seek cannot reach act (capped at reverify)');
  }
});

test('§4A.10: textNotMeaningCaption is worn only when embeddings were NOT used', () => {
  assert.equal(textNotMeaningCaption(false), 'matched by text, not meaning');
  assert.equal(textNotMeaningCaption(true), null);
  assert.equal(textNotMeaningCaption(undefined), null);
  // The REAL fixture used embeddings → no caption.
  assert.equal(textNotMeaningCaption(seek.embeddings_used), null);
});

test('INV-16: a meaning result outside the viewed brain\'s known paths is DROPPED, never rendered', () => {
  // The viewed brain knows only its own snapshot's source_paths.
  const knownPaths = new Set(
    snap.nodes.map((n) => n.provenance.source_path).filter((p): p is string => !!p),
  );
  // A real hit from THIS brain (its file_path is in the snapshot) belongs.
  const inBrain = seek.results.find((r) => r.file_path && knownPaths.has(r.file_path));
  if (inBrain) assert.ok(resultBelongsToBrain(inBrain.file_path, knownPaths), 'own-brain hit is kept');
  // A hit from ANOTHER brain (a path this snapshot never saw) is dropped.
  assert.equal(
    resultBelongsToBrain('some-other-brain/src/foreign.rs', knownPaths),
    false,
    'a foreign-brain path is dropped (INV-16)',
  );
  // Applying the filter to a mixed set drops exactly the foreign rows.
  const mixed = [
    ...seek.results,
    { node_id: 'x', label: 'Foreign', file_path: 'other-brain/x.rs', score: 0.9 },
  ];
  const kept = mixed.filter((r) => resultBelongsToBrain(r.file_path, knownPaths));
  assert.ok(!kept.some((r) => r.file_path === 'other-brain/x.rs'), 'the injected foreign row is not rendered');
});
