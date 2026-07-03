/*
 * Living Tree render smoke — the tree renders from a REAL snapshot fixture, and a
 * file row that carries a memory renders its post-it chip with author+age
 * (PRD Slice-0 render gate). Rendered with react-dom/server (no new deps).
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import TreeRowView from './TreeRowView';
import { buildTree, type TreeRow } from '../../lib/tree';
import type { GraphSnapshot } from '../../lib/snapshot';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '__fixtures__');
const snap = JSON.parse(readFileSync(join(FIX, 'snapshot.compact.json'), 'utf8')) as GraphSnapshot;

function findRow(root: TreeRow, path: string): TreeRow | null {
  let found: TreeRow | null = null;
  const walk = (r: TreeRow) => {
    for (const c of r.children) {
      if (c.path === path) found = c;
      walk(c);
    }
  };
  walk(root);
  return found;
}

const noop = () => {};

test('a real file row with a memory renders its post-it chip and trust dot', () => {
  const root = buildTree(snap);
  const row = findRow(root, 'm1nd-mcp/src/http_server.rs');
  assert.ok(row, 'http_server.rs row exists in the assembled tree');

  const out = renderToStaticMarkup(
    <TreeRowView
      row={row!}
      band="insufficient_evidence"
      breathing={false}
      expanded={false}
      selected={false}
      onToggle={noop}
      onSelect={noop}
      onHover={noop}
      onPostItClick={noop}
    />,
  );

  // The familiar filetree label.
  assert.match(out, /http_server\.rs/);
  // A trust dot (aria-label) is present — this row's band is the honest unknown.
  assert.match(out, /trust: no evidence seen either way yet/);
  assert.match(out, /data-abstain="true"/, 'unknown band dot is abstain-class violet');
  // The post-it chip is present, carrying its state.
  assert.match(out, /data-postit-state="fresh"/);
});

test('a directory row renders an aggregate memory count, not individual chips', () => {
  const root = buildTree(snap);
  // Find a dir that has memories in its subtree.
  let dir: TreeRow | null = null;
  const walk = (r: TreeRow) => {
    for (const c of r.children) {
      if (c.kind === 'dir' && c.subtreePostItCount > 0 && !dir) dir = c;
      walk(c);
    }
  };
  walk(root);
  assert.ok(dir, 'a directory with subtree memories exists');
  const out = renderToStaticMarkup(
    <TreeRowView
      row={dir!}
      band="insufficient_evidence"
      breathing={false}
      expanded
      selected={false}
      onToggle={noop}
      onSelect={noop}
      onHover={noop}
      onPostItClick={noop}
    />,
  );
  // Aggregate count marker (◷ N), not a post-it-state chip.
  assert.match(out, /◷/);
  assert.doesNotMatch(out, /data-postit-state/);
});
