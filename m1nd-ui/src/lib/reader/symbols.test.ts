/*
 * reader/symbols — the outline derived from the graph, against the captured wire
 * shape (reader_snapshot.json). The teeth: only code-symbol nodes of THIS file, in
 * line order; Files/Directories/memory nodes are never outline symbols; fold ranges
 * come straight from the spans.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { fileSymbols, foldRangesFromSymbols, symbolOf } from './symbols';
import type { GraphSnapshot, SnapshotNode } from '../snapshot';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const snap = JSON.parse(readFileSync(join(FIX, 'reader_snapshot.json'), 'utf8')) as GraphSnapshot;

test('fileSymbols returns THIS file’s code symbols, sorted by line_start', () => {
  const syms = fileSymbols(snap, 'repo-alpha/src/graph.rs');
  assert.deepEqual(
    syms.map((s) => [s.label, s.kind, s.lineStart]),
    [
      ['Kind', 'enum', 3],
      ['Graph', 'struct', 9],
      ['insert', 'function', 18],
      ['validate', 'function', 26],
      ['Node', 'struct', 33],
      ['insert', 'function', 39],
    ],
  );
});

test('the File node itself is NOT an outline symbol (only Function/Class/Struct/Enum/Type/Module)', () => {
  const syms = fileSymbols(snap, 'repo-alpha/src/graph.rs');
  assert.ok(!syms.some((s) => s.id === 'file::repo-alpha/src/graph.rs'), 'the file node is excluded');
  // Every returned symbol carries a real span and the owning namespace when present.
  const insert = syms.find((s) => s.label === 'insert' && s.lineStart === 18)!;
  assert.equal(insert.namespace, 'Graph');
  assert.equal(insert.lineEnd, 24);
});

test('symbols are scoped to the open file — a sibling file’s symbols never leak in', () => {
  const store = fileSymbols(snap, 'repo-alpha/src/store.rs').map((s) => s.label);
  assert.deepEqual(store, ['Store', 'open', 'save', 'close']);
  assert.ok(!store.includes('Graph'), 'graph.rs symbols do not appear in store.rs outline');
});

test('a memory (L1GHT) node is never a code symbol, even at node_type Module', () => {
  const light: SnapshotNode = {
    external_id: 'light::light::section::note-md::note-1',
    label: 'note',
    node_type: 7, // Module — the collision the guard must survive
    tags: ['light'],
    last_modified: 0,
    change_frequency: 0,
    provenance: { source_path: 'repo-alpha/src/graph.rs', line_start: 2, line_end: 2, namespace: null, canonical: true },
  };
  assert.equal(symbolOf(light, 'repo-alpha/src/graph.rs'), null);
});

test('foldRangesFromSymbols yields one range per multi-line symbol, none for single-line', () => {
  const syms = fileSymbols(snap, 'repo-alpha/src/graph.rs');
  const folds = foldRangesFromSymbols(syms);
  // All six graph.rs symbols span >1 line in the fixture.
  assert.equal(folds.length, 6);
  const graphFold = folds.find((f) => f.id.endsWith('::struct::Graph'))!;
  assert.deepEqual([graphFold.startLine, graphFold.endLine, graphFold.hiddenCount], [9, 16, 7]);

  const single = foldRangesFromSymbols([
    { id: 's', label: 'x', kind: 'type', lineStart: 5, lineEnd: 5, namespace: null, changeFrequency: 0 },
  ]);
  assert.equal(single.length, 0, 'a single-line symbol is not foldable');
});

test('an empty/absent input is an honest empty outline (never throws)', () => {
  assert.deepEqual(fileSymbols(null, 'x'), []);
  assert.deepEqual(fileSymbols(snap, null), []);
  assert.deepEqual(fileSymbols(snap, 'repo-alpha/does/not/exist.rs'), []);
});
