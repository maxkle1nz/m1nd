/*
 * Living Tree assembly tests — fed by the REAL captured snapshot (compact subset
 * of m1nd's own graph). INV-01: every rendered field maps to a serialized field;
 * post-it provenance matches the snapshot tags byte-for-byte.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { buildTree, buildPostItIndex, flattenVisible } from './tree';
import { isPostItNode, readSourceAgent, readCreatedMs } from './snapshot';
import type { GraphSnapshot } from './snapshot';

const FIX = join(dirname(fileURLToPath(import.meta.url)), '..', '__fixtures__');
const snap = JSON.parse(readFileSync(join(FIX, 'snapshot.compact.json'), 'utf8')) as GraphSnapshot;

test('post-it index anchors memories to the code node their grounded_in edge cites', () => {
  const idx = buildPostItIndex(snap, Date.now());
  // The two real memories in the fixture cite http_server.rs and trust.rs.
  assert.ok(idx.has('m1nd-mcp/src/http_server.rs'), 'http_server.rs must carry a post-it');
  assert.ok(idx.has('m1nd-core/src/trust.rs'), 'trust.rs must carry a post-it');

  const httpPostIts = idx.get('m1nd-mcp/src/http_server.rs')!;
  assert.equal(httpPostIts.length, 1);
  const p = httpPostIts[0];
  // Author comes from the light:source_agent tag on the claim node — byte-for-byte.
  assert.equal(p.sourceAgent, 'agent-refactor');
  assert.ok(p.ageMs != null && p.ageMs >= 0, 'age derived from light:created tag');
  assert.equal(p.state, 'fresh');
});

test('provenance is read from real tags byte-for-byte (INV-01/04)', () => {
  const claim = snap.nodes.find(
    (n) => n.external_id === 'light::light::file::graphsnapshotendpoint-light-md',
  );
  assert.ok(claim, 'the fixture contains the claim node');
  assert.equal(readSourceAgent(claim!), 'agent-refactor');
  const created = readCreatedMs(claim!);
  assert.ok(created != null && created > 0);
  // The tag itself is present verbatim.
  assert.ok(claim!.tags.includes('light:source_agent:agent-refactor'));
  assert.ok(claim!.tags.some((t) => t.startsWith('light:created:')));
});

test('marker-fragment nodes are NOT post-its (client-side filter)', () => {
  const tagNodes = snap.nodes.filter((n) => n.external_id.includes('::tag::'));
  assert.ok(tagNodes.length > 0, 'fixture precondition: has 𝔻/⟁ marker fragments');
  for (const t of tagNodes) {
    assert.equal(isPostItNode(t), false, `marker fragment must be filtered: ${t.external_id}`);
  }
  // Only file/section claim nodes qualify.
  const claims = snap.nodes.filter(isPostItNode);
  assert.ok(claims.every((n) => n.external_id.includes('::file::') || n.external_id.includes('::section::')));
});

test('the tree skeleton is built only from real file nodes with source_path', () => {
  const root = buildTree(snap);
  const all: string[] = [];
  const walk = (r: typeof root) => {
    for (const c of r.children) {
      if (c.kind === 'file') all.push(c.path);
      walk(c);
    }
  };
  walk(root);
  // The cited code files appear as file rows.
  assert.ok(all.includes('m1nd-mcp/src/http_server.rs'));
  assert.ok(all.includes('m1nd-core/src/trust.rs'));
  // Light memory nodes are never tree rows.
  assert.ok(!all.some((p) => p.includes('.light.md')));
});

test('post-its pinned to a file row surface on that row', () => {
  const root = buildTree(snap);
  let httpRow: { postIts: unknown[] } | null = null;
  const walk = (r: typeof root) => {
    for (const c of r.children) {
      if (c.path === 'm1nd-mcp/src/http_server.rs') httpRow = c;
      walk(c);
    }
  };
  walk(root);
  assert.ok(httpRow, 'http_server.rs row exists');
  assert.equal((httpRow as unknown as { postIts: unknown[] }).postIts.length, 1);
});

test('flattenVisible respects expanded dirs and directory aggregate counts roll up', () => {
  const root = buildTree(snap);
  // Nothing expanded → only top-level rows.
  const collapsed = flattenVisible(root, new Set());
  assert.ok(collapsed.length > 0);
  // The root aggregate equals the count of all post-its found.
  const idx = buildPostItIndex(snap, Date.now());
  const totalPostIts = [...idx.values()].reduce((a, l) => a + l.length, 0);
  assert.equal(root.subtreePostItCount, totalPostIts);
});
